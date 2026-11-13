//! Files and directories private to the OS user (ADR 0003): directories
//! 0700, files 0600.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

use anyhow::{bail, Context, Result};

/// Create `dir` (and missing parents) with mode 0700, or check that an
/// existing one is owned by this user and closed to everyone else. An
/// existing directory is never chmod-ed: peerpost refuses rather than
/// guessing that loosening was a mistake.
pub fn ensure_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .with_context(|| format!("cannot create {}", dir.display()))?;
        // The umask can only narrow the mode; set it exactly anyway.
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let meta = std::fs::metadata(dir).with_context(|| format!("cannot stat {}", dir.display()))?;
    if meta.uid() != current_uid() {
        bail!("{} is owned by another user; peerpost will not use it", dir.display());
    }
    if meta.mode() & 0o077 != 0 {
        bail!(
            "{} can be reached by other users (mode {:o}); run `chmod 700` on it",
            dir.display(),
            meta.mode() & 0o777
        );
    }
    Ok(())
}

/// Open `path` for appending, creating it with mode 0600.
pub fn open_append(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

/// The effective uid of this process.
pub fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}
