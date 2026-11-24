//! Small filesystem helpers shared by the code that writes user state: the
//! cache and the agent settings installer.
//!
//! Two kinds of file are written, with different rules:
//!
//! - **track's own state** (the cache): [`atomic_write`] keeps it private,
//!   folders 0o700 and files 0o600 on Unix.
//! - **files that belong to the user** (agent settings, `AGENTS.md`):
//!   [`atomic_write_user_file`] keeps the file's mode, writes through a
//!   symlink to the file it points at, and leaves folder permissions alone.

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

/// How [`write_and_rename`] sets the new file's permissions.
enum Mode {
    /// 0o600 on Unix.
    Private,
    /// These permissions (the replaced file's), or the process default for
    /// a new file.
    Keep(Option<fs::Permissions>),
}

/// Write `content` to a temp file beside `path`, then rename it over `path`.
fn write_and_rename(path: &Path, content: &[u8], durable: bool, mode: Mode) -> Result<()> {
    let temp_path = atomic_write_temp_path(path);
    {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        if matches!(mode, Mode::Private) {
            options.mode(0o600);
        }

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

    match mode {
        Mode::Private => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600));
            }
        }
        Mode::Keep(Some(permissions)) => {
            if let Err(err) = fs::set_permissions(&temp_path, permissions) {
                let _ = fs::remove_file(&temp_path);
                return Err(err).with_context(|| {
                    format!("Failed to keep the permissions of {}", path.display())
                });
            }
        }
        Mode::Keep(None) => {}
    }

    if let Err(err) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(err).with_context(|| {
            format!(
                "Failed to rename {} to {}",
                temp_path.display(),
                path.display()
            )
        });
    }
    Ok(())
}

/// Replace `path` with `content` atomically: write a temp file beside it,
/// then rename. The file is private to the user (0o600 on Unix), and a
/// folder this creates is too (0o700). For track's own state only; see
/// [`atomic_write_user_file`] for files that belong to the user.
///
/// `durable` also fsyncs the temp file first, for data that must survive a
/// sudden power loss. Hot-path metadata can skip it.
pub(crate) fn atomic_write(path: &Path, content: &[u8], durable: bool) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        create_dir_all_secure(parent)?;
    }
    write_and_rename(path, content, durable, Mode::Private)
}

/// The file `path` names once symlinks are followed. A dangling link names
/// the file it would point at.
pub(crate) fn resolve_symlinks(path: &Path) -> Result<PathBuf> {
    // Far more than any real chain; a loop stops here instead of spinning.
    const MAX_LINKS: usize = 40;
    let mut current = path.to_path_buf();
    for _ in 0..MAX_LINKS {
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = fs::read_link(&current)
                    .with_context(|| format!("Failed to read the link {}", current.display()))?;
                current = match current.parent() {
                    Some(parent) if target.is_relative() => parent.join(target),
                    _ => target,
                };
            }
            _ => return Ok(current),
        }
    }
    anyhow::bail!("Too many levels of symbolic links at {}", path.display())
}

/// Whether `path` itself is a symlink.
pub(crate) fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

/// Replace a file that belongs to the user (an agent's settings, an
/// `AGENTS.md`) with `content`, atomically and durably.
///
/// Unlike [`atomic_write`], nothing about the user's setup changes: the file
/// keeps its permissions (a new one gets the process default), a symlink
/// stays a symlink and the file it points at is the one replaced, and
/// folders are created with default permissions and never chmod-ed.
pub(crate) fn atomic_write_user_file(path: &Path, content: &[u8]) -> Result<()> {
    let target = resolve_symlinks(path)?;
    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let permissions = fs::metadata(&target).ok().map(|meta| meta.permissions());
    write_and_rename(&target, content, true, Mode::Keep(permissions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn a_user_file_keeps_its_mode_and_its_folder_keeps_its_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).unwrap();
        fs::set_permissions(&repo, fs::Permissions::from_mode(0o755)).unwrap();
        let file = repo.join("AGENTS.md");
        fs::write(&file, "old\n").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();

        atomic_write_user_file(&file, b"new\n").unwrap();

        assert_eq!(fs::read_to_string(&file).unwrap(), "new\n");
        assert_eq!(mode_of(&file), 0o644);
        assert_eq!(mode_of(&repo), 0o755);
        // No temp file is left beside it.
        assert_eq!(fs::read_dir(&repo).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_new_user_file_and_its_folders_get_the_default_mode_not_a_private_one() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new/.claude/settings.json");

        atomic_write_user_file(&file, b"{}\n").unwrap();

        assert_eq!(fs::read_to_string(&file).unwrap(), "{}\n");
        // Whatever the umask, a private write would give exactly 0o600/0o700.
        let reference = dir.path().join("reference");
        fs::write(&reference, "").unwrap();
        assert_eq!(mode_of(&file), mode_of(&reference));
        let reference_dir = dir.path().join("reference-dir");
        fs::create_dir(&reference_dir).unwrap();
        assert_eq!(mode_of(file.parent().unwrap()), mode_of(&reference_dir));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_user_file_stays_a_link_and_its_target_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let dotfiles = dir.path().join("dotfiles");
        fs::create_dir(&dotfiles).unwrap();
        let target = dotfiles.join("settings.json");
        fs::write(&target, "{}\n").unwrap();
        let claude = dir.path().join(".claude");
        fs::create_dir(&claude).unwrap();
        let link = claude.join("settings.json");
        std::os::unix::fs::symlink("../dotfiles/settings.json", &link).unwrap();

        atomic_write_user_file(&link, b"{\"a\": 1}\n").unwrap();

        assert!(is_symlink(&link));
        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"a\": 1}\n");
        assert_eq!(fs::read_to_string(&link).unwrap(), "{\"a\": 1}\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_link_gets_its_target_created() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink("elsewhere/settings.json", &link).unwrap();

        atomic_write_user_file(&link, b"{}\n").unwrap();

        assert!(is_symlink(&link));
        assert_eq!(
            fs::read_to_string(dir.path().join("elsewhere/settings.json")).unwrap(),
            "{}\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn track_state_stays_private() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cache/index.json");

        atomic_write(&file, b"{}", false).unwrap();

        assert_eq!(mode_of(&file), 0o600);
        assert_eq!(mode_of(file.parent().unwrap()), 0o700);
    }
}
