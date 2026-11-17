//! Small file helpers for `rin generate`.

use std::fs;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use anyhow::{Context, Result};

/// Writes `bytes` unless the file already holds exactly them, so re-running
/// `rin generate` leaves unchanged files (and their bind mounts) alone.
/// Existing files are rewritten in place, keeping their inode for live bind mounts.
pub fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<()> {
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("cannot write {}", path.display()))
}

/// Like [`write_if_changed`], for secrets: readable by you only, from the
/// moment the file exists.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        set_mode(path, 0o600)?;
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("cannot create {}", path.display()))?;
    }
    write_if_changed(path, bytes)
}

pub fn set_mode(path: &Path, mode: u32) -> Result<()> {
    let current = fs::metadata(path)?.permissions().mode() & 0o777;
    if current != mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .with_context(|| format!("cannot chmod {}", path.display()))?;
    }
    Ok(())
}

/// A fresh random secret: 32 random bytes as hex.
pub fn random_hex() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
