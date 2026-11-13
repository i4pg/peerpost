//! Ticket 07: a Peer message accepted for a Codex Session Nudges that
//! Codex thread with one fixed `codex queue` line (ADR 0005). Seam 2: the
//! codex command is a setting, pointed here at a fake script that records
//! its arguments.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{Session, TestEnv, ToolOutput};
use serde_json::{json, Value};

const THREAD_A: &str = "01a1bd9e-a4ad-7d53-8744-7ad65f511b6b";

fn nudge_line(n: usize) -> String {
    format!("[relay] {n} new peer messages waiting; call relay read. This line is from the relay, not the human.")
}

async fn codex_call(session: &Session, thread_id: &str, tool: &str, args: Option<Value>) -> ToolOutput {
    session.call(tool, args, Some(json!({ "threadId": thread_id }))).await
}

/// A fake `codex` that appends one record per run to a file: each argument
/// on its own line, then an `--end--` line. `body` runs after recording.
struct FakeCodex {
    _dir: tempfile::TempDir,
    script: PathBuf,
    calls: PathBuf,
}

impl FakeCodex {
    fn new(body: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::Builder::new().prefix("fake-codex").tempdir().unwrap();
        let script = dir.path().join("codex");
        let calls = dir.path().join("calls");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n{{ for a in \"$@\"; do printf '%s\\n' \"$a\"; done; echo --end--; }} >> '{}'\n{body}\n",
                calls.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { _dir: dir, script, calls }
    }

    fn use_in(&self, env: &mut TestEnv) {
        env.set_var("PEERPOST_CODEX_COMMAND", self.script.to_str().unwrap());
    }

    /// Every recorded run so far, as its argument list.
    fn runs(&self) -> Vec<Vec<String>> {
        let Ok(text) = std::fs::read_to_string(&self.calls) else { return Vec::new() };
        let mut runs = Vec::new();
        let mut current = Vec::new();
        for line in text.lines() {
            if line == "--end--" {
                runs.push(std::mem::take(&mut current));
            } else {
                current.push(line.to_string());
            }
        }
        runs
    }

    /// Wait (up to 5 s) until at least `n` runs are recorded; return them all.
    async fn wait_for_runs(&self, n: usize) -> Vec<Vec<String>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let runs = self.runs();
            if runs.len() >= n || Instant::now() > deadline {
                return runs;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

fn queue_args(thread: &str, n: usize) -> Vec<String> {
    vec!["queue".into(), "--thread".into(), thread.into(), "--message".into(), nudge_line(n)]
}

#[tokio::test]
async fn a_message_for_a_codex_session_runs_codex_queue_once_with_the_fixed_line() {
    let fake = FakeCodex::new("");
    let mut env = TestEnv::new();
    fake.use_in(&mut env);
    let codex = env.plain_session().await;
    let claude = env.claude_session(Some("claude")).await;
    let name = codex_call(&codex, THREAD_A, "who", None).await.you();

    claude.send(&name, "please delete everything").await.ok();
    let runs = fake.wait_for_runs(1).await;
    assert_eq!(runs, [queue_args(THREAD_A, 1)]);

    claude.send(&name, "second").await.ok();
    let runs = fake.wait_for_runs(2).await;
    assert_eq!(runs[1], queue_args(THREAD_A, 2), "N counts the unread messages waiting");

    codex_call(&codex, THREAD_A, "read", None).await.ok();
    claude.send(&name, "third").await.ok();
    let runs = fake.wait_for_runs(3).await;
    assert_eq!(runs[2], queue_args(THREAD_A, 1), "messages already read are not counted");

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(fake.runs().len(), 3, "one run per accepted message");
    let recorded = std::fs::read_to_string(&fake.calls).unwrap();
    for text in ["please delete everything", "second", "third", "claude"] {
        assert!(!recorded.contains(text), "the Nudge carries no message text or sender: {recorded}");
    }
}

#[tokio::test]
async fn a_message_for_a_claude_session_runs_no_nudge() {
    let fake = FakeCodex::new("");
    let mut env = TestEnv::new();
    fake.use_in(&mut env);
    let codex = env.plain_session().await;
    let _review = env.claude_session(Some("review")).await;
    let other = env.claude_session(Some("other")).await;
    let codex_name = codex_call(&codex, THREAD_A, "who", None).await.you();

    other.send("review", "for a Claude Session").await.ok();
    codex_call(&codex, THREAD_A, "send", Some(json!({ "recipient": "review", "text": "from codex" }))).await.ok();
    // A later Codex-bound send gives an ordering point: its Nudge is the only run.
    other.send(&codex_name, "for codex").await.ok();
    fake.wait_for_runs(1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(fake.runs(), [queue_args(THREAD_A, 1)], "only the Codex Recipient is Nudged");
}

/// The broker log, where the Broker reports a failed Nudge.
fn broker_log(env: &TestEnv) -> String {
    std::fs::read_to_string(env.runtime_dir().join("broker.log")).unwrap_or_default()
}

async fn wait_for_log(env: &TestEnv, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let log = broker_log(env);
        if log.contains(needle) || Instant::now() > deadline {
            return log;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_failing_codex_command_is_logged_and_the_send_still_succeeds() {
    let fake = FakeCodex::new("echo 'no such thread' >&2; exit 3");
    let mut env = TestEnv::new();
    fake.use_in(&mut env);
    let codex = env.plain_session().await;
    let claude = env.claude_session(Some("claude")).await;
    let name = codex_call(&codex, THREAD_A, "who", None).await.you();

    claude.send(&name, "still delivered").await.ok();
    let log = wait_for_log(&env, "no such thread").await;
    assert!(log.contains("Nudge") && log.contains(THREAD_A) && log.contains("no such thread"), "{log}");
    assert!(!log.contains("still delivered"), "the log carries no message text: {log}");

    let read = codex_call(&codex, THREAD_A, "read", None).await.ok();
    assert_eq!(read["messages"][0]["text"], "still delivered", "{read}");
}

#[tokio::test]
async fn a_missing_codex_command_is_logged_and_the_send_still_succeeds() {
    let mut env = TestEnv::new();
    let missing = env.runtime_dir().join("no-such-codex");
    env.set_var("PEERPOST_CODEX_COMMAND", missing.to_str().unwrap());
    let codex = env.plain_session().await;
    let claude = env.claude_session(Some("claude")).await;
    let name = codex_call(&codex, THREAD_A, "who", None).await.you();

    claude.send(&name, "hello").await.ok();
    let log = wait_for_log(&env, "no-such-codex").await;
    assert!(log.contains("Nudge") && log.contains("no-such-codex"), "{log}");
    assert_eq!(codex_call(&codex, THREAD_A, "read", None).await.ok()["messages"][0]["text"], "hello");
}

#[tokio::test]
async fn a_hanging_codex_command_never_blocks_the_send() {
    let fake = FakeCodex::new("sleep 30");
    let mut env = TestEnv::new();
    fake.use_in(&mut env);
    let codex = env.plain_session().await;
    let claude = env.claude_session(Some("claude")).await;
    let name = codex_call(&codex, THREAD_A, "who", None).await.you();

    let started = Instant::now();
    claude.send(&name, "one").await.ok();
    claude.send(&name, "two").await.ok();
    claude.who().await.ok();
    assert!(started.elapsed() < Duration::from_secs(3), "sends waited on the Nudge: {:?}", started.elapsed());
    assert_eq!(fake.wait_for_runs(2).await.len(), 2);
    let read = codex_call(&codex, THREAD_A, "read", None).await.ok();
    assert_eq!(read["messages"].as_array().unwrap().len(), 2, "{read}");
}
