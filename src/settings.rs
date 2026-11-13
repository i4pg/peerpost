//! Settings, read once at start. Every value can be overridden by an
//! environment variable so tests (and Humans) can run peerpost hermetically.
//! No default names any one installation.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};

/// Overrides the per-user runtime directory (Broker socket, broker log).
pub const ENV_RUNTIME_DIR: &str = "PEERPOST_RUNTIME_DIR";
/// Overrides the per-user data directory (Message log).
pub const ENV_DATA_DIR: &str = "PEERPOST_DATA_DIR";
/// How long the Broker keeps running with no client connected, in
/// milliseconds.
pub const ENV_GRACE_MS: &str = "PEERPOST_GRACE_MS";

const DEFAULT_GRACE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Settings {
    pub runtime_dir: PathBuf,
    pub data_dir: PathBuf,
    /// The Broker exits once no client has been connected for this long.
    pub grace: Duration,
}

impl Settings {
    pub fn from_env() -> Result<Self> {
        Ok(Self { runtime_dir: runtime_dir()?, data_dir: data_dir()?, grace: grace()? })
    }

    pub fn socket_path(&self) -> PathBuf {
        self.runtime_dir.join("broker.sock")
    }

    /// Held (flock) by the running Broker for its whole life; it holds the
    /// Broker's process id for the Human's benefit.
    pub fn broker_lock_path(&self) -> PathBuf {
        self.runtime_dir.join("broker.lock")
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

fn grace() -> Result<Duration> {
    match std::env::var(ENV_GRACE_MS) {
        Ok(v) if !v.is_empty() => v
            .parse::<u64>()
            .map(Duration::from_millis)
            .with_context(|| format!("{ENV_GRACE_MS} must be a whole number of milliseconds, got `{v}`")),
        _ => Ok(DEFAULT_GRACE),
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
