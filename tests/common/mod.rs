//! Shared Seam 1 harness: drives the built `peerpost` binary as a stdio MCP
//! client would, inside a throwaway runtime and data directory.
#![allow(dead_code)] // each test file uses a different subset

use std::path::{Path, PathBuf};

use rmcp::{
    model::{CallToolRequestParams, RequestMetaObject},
    service::RunningService,
    transport::TokioChildProcess,
    RoleClient, ServiceExt,
};
use serde_json::{Map, Value};
use tempfile::TempDir;
use tokio::process::Command;

/// Environment variables a test may set on a spawned peerpost process.
/// Everything from the outer environment that could change peerpost's
/// behaviour is removed first, so tests are hermetic.
const SCRUBBED_VARS: &[&str] = &[
    "CLAUDECODE",
    "RELAY_NAME",
    "XDG_RUNTIME_DIR",
    "XDG_DATA_HOME",
    "XDG_CONFIG_HOME",
    "PEERPOST_LOG_KEEP",
    "PEERPOST_MAX_MESSAGE_BYTES",
    "PEERPOST_MAX_UNREAD",
    "PEERPOST_GRACE_MS",
];

/// A private runtime and data directory; every Session and Broker started
/// from one `TestEnv` shares them. Dropping it removes both, which also makes
/// the Broker exit.
pub struct TestEnv {
    root: TempDir,
    /// Extra variables set on every command, after the scrub.
    vars: Vec<(String, String)>,
}

impl TestEnv {
    pub fn new() -> Self {
        Self { root: tempfile::Builder::new().prefix("pp").tempdir().expect("tempdir"), vars: Vec::new() }
    }

    /// Set `name=value` on every peerpost process this env starts from now
    /// on (Broker included, as it inherits the starting process's env).
    pub fn set_var(&mut self, name: &str, value: &str) {
        self.vars.push((name.into(), value.into()));
    }

    /// The OS user's config directory as peerpost sees it
    /// (`$XDG_CONFIG_HOME`); empty unless a test writes into it.
    pub fn config_home(&self) -> PathBuf {
        self.root.path().join("config")
    }

    /// Write peerpost's settings file (`<config home>/peerpost/config.toml`).
    pub fn write_settings_file(&self, contents: &str) {
        let dir = self.config_home().join("peerpost");
        std::fs::create_dir_all(&dir).expect("create config dir");
        std::fs::write(dir.join("config.toml"), contents).expect("write settings file");
    }

    /// Process ids of the live Brokers using this env's runtime directory
    /// (found through `/proc`, so Linux only).
    pub fn broker_pids(&self) -> Vec<u32> {
        let wanted = format!("PEERPOST_RUNTIME_DIR={}", self.runtime_dir().display()).into_bytes();
        let mut pids = Vec::new();
        for entry in std::fs::read_dir("/proc").expect("/proc").flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
            let (Ok(cmdline), Ok(environ)) =
                (std::fs::read(entry.path().join("cmdline")), std::fs::read(entry.path().join("environ")))
            else {
                continue;
            };
            let is_broker = cmdline.split(|b| *b == 0).nth(1) == Some(b"broker".as_slice());
            if is_broker && environ.split(|b| *b == 0).any(|v| v == wanted.as_slice()) {
                pids.push(pid);
            }
        }
        pids
    }

    /// The runtime directory peerpost is told to use (created by peerpost).
    pub fn runtime_dir(&self) -> PathBuf {
        self.root.path().join("run")
    }

    /// The data directory peerpost is told to use (created by peerpost).
    pub fn data_dir(&self) -> PathBuf {
        self.root.path().join("data")
    }

    /// A `Command` for the peerpost binary with this env's directories set
    /// and the outer environment's peerpost-relevant variables removed.
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_peerpost"));
        for var in SCRUBBED_VARS {
            cmd.env_remove(var);
        }
        cmd.env("PEERPOST_RUNTIME_DIR", self.runtime_dir());
        cmd.env("PEERPOST_DATA_DIR", self.data_dir());
        cmd.env("XDG_CONFIG_HOME", self.config_home());
        for (name, value) in &self.vars {
            cmd.env(name, value);
        }
        cmd
    }

    /// Run `peerpost <args>` to completion and return its output.
    pub async fn run(&self, args: &[&str]) -> std::process::Output {
        let mut cmd = self.command();
        cmd.args(args).stdin(std::process::Stdio::null()).kill_on_drop(true);
        tokio::time::timeout(std::time::Duration::from_secs(10), cmd.output())
            .await
            .expect("peerpost command timed out")
            .expect("run peerpost")
    }

    /// The Message log file, as the README names it.
    pub fn message_log(&self) -> PathBuf {
        self.data_dir().join("messages.jsonl")
    }

    /// Start a Broker by hand, so the test can stop it.
    pub async fn start_broker(&self) -> tokio::process::Child {
        let mut cmd = self.command();
        cmd.arg("broker").stdin(std::process::Stdio::null()).kill_on_drop(true);
        let child = cmd.spawn().expect("spawn broker");
        let socket = self.runtime_dir().join("broker.sock");
        for _ in 0..200 {
            if tokio::net::UnixStream::connect(&socket).await.is_ok() {
                return child;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("the Broker did not answer on {}", socket.display());
    }

    /// Start a Claude Session's MCP server (`CLAUDECODE=1`), optionally with
    /// `RELAY_NAME`, and connect to it as an MCP client.
    pub async fn claude_session(&self, relay_name: Option<&str>) -> Session {
        let mut cmd = self.command();
        cmd.env("CLAUDECODE", "1");
        if let Some(name) = relay_name {
            cmd.env("RELAY_NAME", name);
        }
        Session::spawn(cmd).await
    }

    /// Start an MCP server with no product markers at all (neither
    /// `CLAUDECODE` nor any `_meta.threadId` unless the test adds one).
    pub async fn plain_session(&self) -> Session {
        Session::spawn(self.command()).await
    }
}

/// One MCP client connected to one peerpost MCP server process.
pub struct Session {
    client: RunningService<RoleClient, ()>,
}

/// What a tool call returned: the JSON body of its text content, and whether
/// the server flagged it as a tool error.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub is_error: bool,
    pub text: String,
}

impl ToolOutput {
    /// The text content parsed as JSON; panics if it is not JSON.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or_else(|e| panic!("tool output is not JSON ({e}): {}", self.text))
    }

    /// The caller's own Sender name, as every tool result states it.
    pub fn you(&self) -> String {
        self.json()["you"].as_str().expect("`you` in tool result").to_string()
    }

    /// Panics unless this was a successful call; returns the JSON body.
    pub fn ok(self) -> Value {
        assert!(!self.is_error, "expected success, got tool error: {}", self.text);
        self.json()
    }
}

impl Session {
    async fn spawn(cmd: Command) -> Session {
        let transport = TokioChildProcess::new(cmd).expect("spawn peerpost");
        let client = ().serve(transport).await.expect("MCP initialize");
        Session { client }
    }

    /// Call any tool with optional arguments and an optional request `_meta`.
    pub async fn call(&self, tool: &str, args: Option<Value>, meta: Option<Value>) -> ToolOutput {
        let mut params = CallToolRequestParams::new(tool.to_string());
        params.arguments = args.map(|v| match v {
            Value::Object(m) => m,
            other => panic!("tool arguments must be an object, got {other}"),
        });
        if let Some(meta) = meta {
            let obj: Map<String, Value> = match meta {
                Value::Object(m) => m,
                other => panic!("_meta must be an object, got {other}"),
            };
            params.meta = Some(serde_json::from_value::<RequestMetaObject>(Value::Object(obj)).unwrap());
        }
        let result = self.client.call_tool(params).await.expect("tools/call transport");
        let text =
            result.content.iter().filter_map(|c| c.as_text().map(|t| t.text.clone())).collect::<Vec<_>>().join("");
        ToolOutput { is_error: result.is_error.unwrap_or(false), text }
    }

    pub async fn who(&self) -> ToolOutput {
        self.call("who", None, None).await
    }

    pub async fn send(&self, recipient: &str, text: &str) -> ToolOutput {
        self.call("send", Some(serde_json::json!({"recipient": recipient, "text": text})), None).await
    }

    pub async fn read(&self) -> ToolOutput {
        self.call("read", None, None).await
    }

    /// The names of the tools this server lists.
    pub async fn tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> =
            self.client.list_all_tools().await.expect("tools/list").into_iter().map(|t| t.name.to_string()).collect();
        names.sort();
        names
    }

    /// Close the MCP connection, as Claude Code does when a Session ends.
    pub async fn close(self) {
        let _ = self.client.cancel().await;
    }
}

/// Unix permission bits of a path (the low 9 bits).
pub fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).expect("stat").permissions().mode() & 0o777
}

impl TestEnv {
    /// Whether a Broker answers on this env's socket right now.
    pub async fn broker_answers(&self) -> bool {
        tokio::net::UnixStream::connect(self.runtime_dir().join("broker.sock")).await.is_ok()
    }

    /// Wait up to 5 s for a Broker to answer; panics if none does.
    pub async fn wait_for_broker(&self) {
        for _ in 0..200 {
            if self.broker_answers().await {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("no Broker answered");
    }

    /// Start `peerpost watch <name>` as Claude Code's Monitor tool would, with
    /// its stdout captured line by line.
    pub fn watch(&self, name: &str) -> Watcher {
        let mut cmd = self.command();
        cmd.args(["watch", name])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().expect("spawn peerpost watch");
        let stdout = child.stdout.take().expect("watch stdout");
        let lines = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(stdout));
        Watcher { child, lines }
    }
}

/// A running `peerpost watch`.
pub struct Watcher {
    child: tokio::process::Child,
    lines: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
}

impl Watcher {
    /// The next line the Watcher prints; panics if none comes within 10 s.
    pub async fn next_line(&mut self) -> String {
        tokio::time::timeout(std::time::Duration::from_secs(10), self.lines.next_line())
            .await
            .expect("the Watcher printed no line within 10 s")
            .expect("read watch stdout")
            .expect("the Watcher ended without printing a line")
    }

    /// Panics if the Watcher prints a line within `ms` milliseconds.
    pub async fn assert_quiet_for(&mut self, ms: u64) {
        if let Ok(line) = tokio::time::timeout(std::time::Duration::from_millis(ms), self.lines.next_line()).await {
            panic!("the Watcher should stay quiet, but printed {line:?}");
        }
    }

    /// Send `signal` (such as `libc::SIGTERM`) and wait for the Watcher to
    /// end; returns its exit status and whatever it printed after the
    /// last line read.
    pub async fn stop_with(mut self, signal: i32) -> (std::process::ExitStatus, Vec<String>) {
        let pid = self.child.id().expect("watcher still running") as i32;
        // SAFETY: plain kill(2) on a child this test owns.
        unsafe { libc::kill(pid, signal) };
        let status = tokio::time::timeout(std::time::Duration::from_secs(10), self.child.wait())
            .await
            .expect("the Watcher did not stop within 10 s")
            .expect("wait for watcher");
        let mut rest = Vec::new();
        while let Ok(Some(line)) = self.lines.next_line().await {
            rest.push(line);
        }
        (status, rest)
    }
}
