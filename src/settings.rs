//! Settings, read once at start. Every value can be overridden by an
//! environment variable so tests (and Humans) can run peerpost hermetically.
//! No default names any one installation.

use std::path::PathBuf;

use anyhow::{Context, Result};

/// Overrides the per-user runtime directory (Broker socket, broker log).
pub const ENV_RUNTIME_DIR: &str = "PEERPOST_RUNTIME_DIR";
/// Overrides the per-user data directory (Message log).
pub const ENV_DATA_DIR: &str = "PEERPOST_DATA_DIR";

#[derive(Debug, Clone)]
pub struct Settings {
    pub runtime_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Settings {
    pub fn from_env() -> Result<Self> {
        Ok(Self { runtime_dir: runtime_dir()?, data_dir: data_dir()? })
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
    // SAFETY: geteuid has no preconditions and cannot fail.
    let uid = unsafe { libc::geteuid() };
    Ok(std::env::temp_dir().join(format!("peerpost-{uid}")))
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
