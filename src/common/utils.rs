use std::fs;
use std::io::Write;
use std::path::Path;

use crate::common::error::{Error, Result};

/// Replace a file through a synced temporary sibling and `rename()`.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write_with_mode(path, bytes, None)
}

/// Atomically replace a private file, using owner-only permissions on Unix.
pub fn atomic_write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write_with_mode(path, bytes, Some(0o600))
}

fn atomic_write_with_mode(path: &Path, bytes: &[u8], mode: Option<u32>) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Io(std::io::Error::other("missing parent directory")))?;
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let tmp = parent.join(format!(".tmp-{name}-{}", std::process::id()));
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(mode);
        }
        let mut file = options.open(&tmp)?;
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(mode))?;
        }
        #[cfg(not(unix))]
        let _ = mode;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// SHA-256 in the lowercase hexadecimal format used by widget registries.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
