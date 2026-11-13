//! Settings, read once at start. Every value can be overridden by an
//! environment variable so tests (and Humans) can run peerpost hermetically.
//! No default names any one installation.

use std::path::PathBuf;

use anyhow::{Context, Result};

/// Overrides the per-user runtime directory (Broker socket, broker log).
pub const ENV_RUNTIME_DIR: &str = "PEERPOST_RUNTIME_DIR";
/// Overrides the per-user data directory (Message log).
pub const ENV_DATA_DIR: &str = "PEERPOST_DATA_DIR";
/// How many Peer messages the Message log keeps; older ones are dropped.
pub const ENV_LOG_KEEP: &str = "PEERPOST_LOG_KEEP";
/// The default for [`ENV_LOG_KEEP`].
pub const DEFAULT_LOG_KEEP: usize = 10_000;

#[derive(Debug, Clone)]
pub struct Settings {
    pub runtime_dir: PathBuf,
    pub data_dir: PathBuf,
    /// The Message log keeps only this many newest Peer messages (at least 1).
    pub log_keep: usize,
}

impl Settings {
    pub fn from_env() -> Result<Self> {
        Ok(Self { runtime_dir: runtime_dir()?, data_dir: data_dir()?, log_keep: log_keep()? })
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

/// `$PEERPOST_LOG_KEEP`, else [`DEFAULT_LOG_KEEP`]; must be a whole number of
/// at least 1.
fn log_keep() -> Result<usize> {
    let Some(raw) = std::env::var_os(ENV_LOG_KEEP).filter(|v| !v.is_empty()) else {
        return Ok(DEFAULT_LOG_KEEP);
    };
    match raw.to_str().and_then(|v| v.trim().parse::<usize>().ok()) {
        Some(n) if n >= 1 => Ok(n),
        _ => anyhow::bail!("{ENV_LOG_KEEP} must be a whole number of at least 1, got {raw:?}"),
    }
}
