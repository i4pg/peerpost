//! Settings, read once at start. Tunable values come from the settings file
//! (`config.toml` in the OS user's config directory, see [`settings_file_path`])
//! and every value can be overridden by an environment variable, which wins,
//! so tests (and Humans) can run peerpost hermetically. No default names any
//! one installation.

use std::path::{Path, PathBuf};

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

/// How many Peer messages the Message log keeps; older ones are dropped.
pub const ENV_LOG_KEEP: &str = "PEERPOST_LOG_KEEP";

/// The default for [`ENV_LOG_KEEP`].
pub const DEFAULT_LOG_KEEP: usize = 10_000;
const DEFAULT_MAX_MESSAGE_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_UNREAD: usize = 100;

#[derive(Debug, Clone)]
pub struct Settings {
    pub runtime_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Largest Peer message text the Broker accepts, in bytes.
    pub max_message_bytes: usize,
    /// Most unread Peer messages one Recipient may hold.
    pub max_unread: usize,
    /// The Message log keeps only this many newest Peer messages (at least 1).
    pub log_keep: usize,
}

/// The settings file's keys; every one is optional.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsFile {
    max_message_bytes: Option<usize>,
    max_unread: Option<usize>,
    log_keep: Option<usize>,
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
            max_message_bytes: limit(
                ENV_MAX_MESSAGE_BYTES,
                "max_message_bytes",
                file.max_message_bytes,
                DEFAULT_MAX_MESSAGE_BYTES,
            )?,
            max_unread: limit(ENV_MAX_UNREAD, "max_unread", file.max_unread, DEFAULT_MAX_UNREAD)?,
            log_keep: limit(ENV_LOG_KEEP, "log_keep", file.log_keep, DEFAULT_LOG_KEEP)?,
        })
    }

    pub fn socket_path(&self) -> PathBuf {
        self.runtime_dir.join("broker.sock")
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

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// `$PEERPOST_RUNTIME_DIR`, else `$XDG_RUNTIME_DIR/peerpost`, else
/// `<temp dir>/peerpost-<uid>`.
fn runtime_dir() -> Result<PathBuf> {
    if let Some(dir) = env_path(ENV_RUNTIME_DIR) {
        return Ok(dir);
    }
    if let Some(dir) = env_path("XDG_RUNTIME_DIR") {
        return Ok(dir.join("peerpost"));
    }
    Ok(std::env::temp_dir().join(format!("peerpost-{}", crate::private_fs::current_uid())))
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
