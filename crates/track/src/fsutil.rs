//! Small filesystem helpers shared by the code that writes user state: the
//! cache and the agent settings installer.

use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Creates a directory and its parents with secure permissions (0o700 on Unix)
pub(crate) fn create_dir_all_secure(path: &Path) -> Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// Temp file used to atomically replace `path`. The process id keeps
/// concurrent `track` runs from truncating each other's half-written file.
pub(crate) fn atomic_write_temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

/// Replace `path` with `content` atomically: write a temp file beside it,
/// then rename. The file is private to the user (0o600 on Unix).
///
/// `durable` also fsyncs the temp file first, for data that must survive a
/// sudden power loss. Hot-path metadata can skip it.
pub(crate) fn atomic_write(path: &Path, content: &[u8], durable: bool) -> Result<()> {
    let temp_path = atomic_write_temp_path(path);

    // Create parent directory if needed
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        create_dir_all_secure(parent)?;
    }

    {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);

        let mut file = options
            .open(&temp_path)
            .with_context(|| format!("Failed to create temp file: {}", temp_path.display()))?;

        file.write_all(content)
            .with_context(|| format!("Failed to write to temp file: {}", temp_path.display()))?;
        if durable {
            file.sync_all()
                .with_context(|| format!("Failed to sync temp file: {}", temp_path.display()))?;
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600));
    }

    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}
