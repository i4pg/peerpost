//! Settings, read once at start. Tunable values come from the settings file
//! (`config.toml` in the OS user's config directory, see [`settings_file_path`])
//! and every value can be overridden by an environment variable, which wins,
//! so tests (and Humans) can run peerpost hermetically. No default names any
//! one installation.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Overrides the per-user runtime directory (Broker socket, broker log).
pub const ENV_RUNTIME_DIR: &str = "PEERPOST_RUNTIME_DIR";
/// Overrides the per-user data directory (Message log).
pub const ENV_DATA_DIR: &str = "PEERPOST_DATA_DIR";
/// Overrides the largest Peer message, in bytes.
pub const ENV_MAX_MESSAGE_BYTES: &str = "PEERPOST_MAX_MESSAGE_BYTES";
/// Overrides how many unread Peer messages one Recipient may hold.
pub const ENV_MAX_UNREAD: &str = "PEERPOST_MAX_UNREAD";
/// How long the Broker keeps running with no client connected, in
/// milliseconds.
pub const ENV_GRACE_MS: &str = "PEERPOST_GRACE_MS";

const DEFAULT_GRACE: Duration = Duration::from_secs(30);

/// How many Peer messages the Message log keeps; older ones are dropped.
pub const ENV_LOG_KEEP: &str = "PEERPOST_LOG_KEEP";

/// The default for [`ENV_LOG_KEEP`].
pub const DEFAULT_LOG_KEEP: usize = 10_000;
const DEFAULT_MAX_MESSAGE_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_UNREAD: usize = 100;
/// The `codex` program the Broker runs to Nudge a Codex Session.
pub const ENV_CODEX_COMMAND: &str = "PEERPOST_CODEX_COMMAND";
/// The default for [`ENV_CODEX_COMMAND`]: `codex`, found on `PATH`.
pub const DEFAULT_CODEX_COMMAND: &str = "codex";

#[derive(Debug, Clone)]
pub struct Settings {
    pub runtime_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Largest Peer message text the Broker accepts, in bytes.
    pub max_message_bytes: usize,
    /// Most unread Peer messages one Recipient may hold.
    pub max_unread: usize,
    /// The Broker exits once no client has been connected for this long.
    pub grace: Duration,
    /// The Message log keeps only this many newest Peer messages (at least 1).
    pub log_keep: usize,
    /// The `codex` program a Nudge runs (`<it> queue --thread <id> --message <line>`).
    pub codex_command: PathBuf,
}

/// The settings file's keys; every one is optional.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsFile {
    max_message_bytes: Option<usize>,
    max_unread: Option<usize>,
    log_keep: Option<usize>,
    grace_ms: Option<u64>,
    codex_command: Option<PathBuf>,
}

impl Settings {
    /// Read the settings file (if any) and the environment.
    pub fn from_env() -> Result<Self> {
        let file = match settings_file_path() {
            Some(path) => read_settings_file(&path)?,
            None => SettingsFile::default(),
        };
        Ok(Self {
            runtime_dir: runtime_dir()?,
            data_dir: data_dir()?,
            grace: grace(file.grace_ms)?,
            max_message_bytes: limit(
                ENV_MAX_MESSAGE_BYTES,
                "max_message_bytes",
                file.max_message_bytes,
                DEFAULT_MAX_MESSAGE_BYTES,
            )?,
            max_unread: limit(ENV_MAX_UNREAD, "max_unread", file.max_unread, DEFAULT_MAX_UNREAD)?,
            log_keep: limit(ENV_LOG_KEEP, "log_keep", file.log_keep, DEFAULT_LOG_KEEP)?,
            codex_command: env_path(ENV_CODEX_COMMAND)
                .or(file.codex_command)
                .unwrap_or_else(|| DEFAULT_CODEX_COMMAND.into()),
        })
    }

    pub fn socket_path(&self) -> PathBuf {
        self.runtime_dir.join("broker.sock")
    }

    /// Held (flock) by the running Broker for its whole life: one guards the
    /// socket, the other the Message log.
    pub fn broker_lock_paths(&self) -> [PathBuf; 2] {
        [self.runtime_dir.join("broker.lock"), self.data_dir.join("broker.lock")]
    }

    /// Held (flock) by a client while it starts a Broker, so clients that
    /// find no Broker at the same moment start only one.
    pub fn start_lock_path(&self) -> PathBuf {
        self.runtime_dir.join("start.lock")
    }

    pub fn broker_log_path(&self) -> PathBuf {
        self.runtime_dir.join("broker.log")
    }

    pub fn message_log_path(&self) -> PathBuf {
        self.data_dir.join("messages.jsonl")
    }
}

/// `$XDG_CONFIG_HOME/peerpost/config.toml`, else
/// `$HOME/.config/peerpost/config.toml`.
fn settings_file_path() -> Option<PathBuf> {
    let config_home = env_path("XDG_CONFIG_HOME").or_else(|| env_path("HOME").map(|home| home.join(".config")))?;
    Some(config_home.join("peerpost").join("config.toml"))
}

/// A missing file means no settings; a file that does not parse is an error.
fn read_settings_file(path: &Path) -> Result<SettingsFile> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SettingsFile::default()),
        Err(e) => return Err(e).with_context(|| format!("cannot read the settings file {}", path.display())),
    };
    toml::from_str(&text).with_context(|| format!("bad settings file {}", path.display()))
}

/// A limit: the environment variable `env_name` if set, else the settings
/// file's `file_key`, else `default`. A limit must be a whole number of at
/// least 1.
fn limit(env_name: &str, file_key: &str, from_file: Option<usize>, default: usize) -> Result<usize> {
    let (value, source) = match std::env::var(env_name).ok().filter(|v| !v.is_empty()) {
        Some(text) => (text.trim().parse::<usize>().ok(), format!("{env_name} (`{text}`)")),
        None => match from_file {
            Some(n) => (Some(n), format!("`{file_key}` in the settings file")),
            None => return Ok(default),
        },
    };
    match value {
        Some(n) if n >= 1 => Ok(n),
        _ => bail!("{source} must be a whole number of at least 1"),
    }
}

/// The grace period must outlast a client's own start of the Broker, or the
/// Broker could stop before the client that started it connects.
const MIN_GRACE: Duration = Duration::from_millis(100);

/// The grace period: [`ENV_GRACE_MS`] if set, else the settings file's
/// `grace_ms`, else the default.
fn grace(from_file: Option<u64>) -> Result<Duration> {
    let (value, source) = match std::env::var(ENV_GRACE_MS).ok().filter(|v| !v.is_empty()) {
        Some(v) => (v.trim().parse::<u64>().ok(), format!("{ENV_GRACE_MS} (`{v}`)")),
        None => match from_file {
            Some(ms) => (Some(ms), "`grace_ms` in the settings file".to_string()),
            None => return Ok(DEFAULT_GRACE),
        },
    };
    match value.map(Duration::from_millis) {
        Some(grace) if grace >= MIN_GRACE => Ok(grace),
        _ => bail!("{source} must be a whole number of milliseconds, at least {}", MIN_GRACE.as_millis()),
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// `$PEERPOST_RUNTIME_DIR`, else [`default_runtime_dir`].
fn runtime_dir() -> Result<PathBuf> {
    if let Some(dir) = env_path(ENV_RUNTIME_DIR) {
        return Ok(dir);
    }
    let uid = crate::private_fs::current_uid();
    let systemd = PathBuf::from(format!("/run/user/{uid}"));
    let systemd = systemd.is_dir().then_some(systemd);
    Ok(default_runtime_dir(env_path("XDG_RUNTIME_DIR"), systemd, &std::env::temp_dir(), uid))
}

/// `$XDG_RUNTIME_DIR/peerpost`, else `/run/user/<uid>/peerpost` when that
/// directory exists, else `<temp dir>/peerpost-<uid>`.
///
/// The middle step matters: Codex CLI starts MCP servers with only a few
/// environment variables, never `XDG_RUNTIME_DIR`, while Claude Code passes
/// it on. On a systemd system it is `/run/user/<uid>`, so both still meet at
/// one Broker.
fn default_runtime_dir(
    xdg_runtime_dir: Option<PathBuf>,
    systemd_user_dir: Option<PathBuf>,
    temp_dir: &Path,
    uid: u32,
) -> PathBuf {
    match xdg_runtime_dir.or(systemd_user_dir) {
        Some(dir) => dir.join("peerpost"),
        None => temp_dir.join(format!("peerpost-{uid}")),
    }
}

/// `$PEERPOST_DATA_DIR`, else `$XDG_DATA_HOME/peerpost`, else
/// `$HOME/.local/share/peerpost`.
fn data_dir() -> Result<PathBuf> {
    if let Some(dir) = env_path(ENV_DATA_DIR) {
        return Ok(dir);
    }
    if let Some(dir) = env_path("XDG_DATA_HOME") {
        return Ok(dir.join("peerpost"));
    }
    let home = env_path("HOME").context("cannot find a data directory: set HOME or PEERPOST_DATA_DIR")?;
    Ok(home.join(".local").join("share").join("peerpost"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Codex CLI starts MCP servers with only a few environment variables
    // (not XDG_RUNTIME_DIR), while Claude Code passes them all; both must
    // still find the one Broker. Not testable at Seam 1 without touching
    // the real /run/user directory, so the choice is tested here.
    #[test]
    fn a_session_without_xdg_runtime_dir_finds_the_same_runtime_dir_on_a_systemd_system() {
        let run_user = PathBuf::from("/run/user/1000");
        let tmp = PathBuf::from("/tmp");
        let claude = default_runtime_dir(Some(run_user.clone()), Some(run_user.clone()), &tmp, 1000);
        let codex = default_runtime_dir(None, Some(run_user.clone()), &tmp, 1000);
        assert_eq!(claude, PathBuf::from("/run/user/1000/peerpost"));
        assert_eq!(codex, claude);
    }

    #[test]
    fn xdg_runtime_dir_wins_and_the_temp_dir_is_the_last_resort() {
        let tmp = PathBuf::from("/tmp");
        assert_eq!(
            default_runtime_dir(Some("/custom".into()), Some("/run/user/7".into()), &tmp, 7),
            PathBuf::from("/custom/peerpost")
        );
        assert_eq!(default_runtime_dir(None, None, &tmp, 7), PathBuf::from("/tmp/peerpost-7"));
    }
}
