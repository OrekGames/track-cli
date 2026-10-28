pub mod apply;
pub mod article;
pub(crate) mod attachments;
pub mod bundle;
pub mod cache;
pub mod config;
pub mod context;
pub mod doctor;
pub mod eval;
pub mod field;
pub mod init;
pub(crate) mod inspect;
pub mod issue;
pub mod open;
pub mod project;
pub mod tags;
pub mod workflow;

use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;

/// Replace `secret` with `<redacted>` so wrapper/error copy cannot echo tokens.
pub(crate) fn without_secret(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "<redacted>")
    }
}

/// Resolve text content from an inline string or a file path.
///
/// If `body_file` is `Some`, reads from the given path (use `"-"` for stdin).
/// Otherwise returns `inline` as-is. Trims trailing line endings (`\n` and
/// Windows `\r\n`) from file/stdin content so that the editor-appended newline
/// isn't sent to the API.
pub(crate) fn resolve_body(
    inline: Option<&str>,
    body_file: Option<&Path>,
) -> Result<Option<String>> {
    if let Some(path) = body_file {
        let content = if path.as_os_str() == "-" {
            let mut buf = String::new();
            std::io::stdin()
                .lock()
                .read_to_string(&mut buf)
                .context("Failed to read from stdin")?;
            buf
        } else {
            if !path.exists() {
                anyhow::bail!("File '{}' does not exist", path.display());
            }
            if !path.is_file() {
                anyhow::bail!("Path '{}' is not a file", path.display());
            }
            std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read '{}'", path.display()))?
        };
        Ok(Some(content.trim_end_matches(['\r', '\n']).to_string()))
    } else {
        Ok(inline.map(String::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Write `content` to a file in a fresh temp dir. Unique per test so
    /// concurrent test runs on one machine never share or delete each
    /// other's files.
    fn temp_file(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("body.txt");
        std::fs::write(&file, content).unwrap();
        (dir, file)
    }

    #[test]
    fn resolve_body_returns_none_when_both_none() {
        let result = resolve_body(None, None).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn resolve_body_returns_inline_when_no_file() {
        let result = resolve_body(Some("hello world"), None).unwrap();
        assert_eq!(result.as_deref(), Some("hello world"));
    }

    #[test]
    fn resolve_body_reads_file_content() {
        let (_dir, file) = temp_file("## Title\n\nBody text\n");

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("## Title\n\nBody text"));
    }

    #[test]
    fn resolve_body_trims_trailing_newline_from_file() {
        let (_dir, file) = temp_file("content\n");

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("content"));
    }

    #[test]
    fn resolve_body_trims_trailing_crlf_from_file() {
        let (_dir, file) = temp_file("line1\r\nline2\r\n");

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("line1\r\nline2"));
    }

    #[test]
    fn resolve_body_trims_multiple_trailing_newlines() {
        let (_dir, file) = temp_file("content\n\n\n");

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("content"));
    }

    #[test]
    fn resolve_body_preserves_internal_newlines() {
        let (_dir, file) = temp_file("line1\nline2\nline3\n");

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("line1\nline2\nline3"));
    }

    #[test]
    fn resolve_body_file_takes_precedence_over_inline() {
        let (_dir, file) = temp_file("from file\n");

        let result = resolve_body(Some("from inline"), Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("from file"));
    }

    #[test]
    fn resolve_body_errors_on_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.md");
        let result = resolve_body(None, Some(&path));
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("does not exist"),
            "error should mention file does not exist"
        );
    }

    #[test]
    fn resolve_body_errors_on_directory() {
        let dir = tempfile::TempDir::new().unwrap();

        let result = resolve_body(None, Some(dir.path()));
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("is not a file"),
            "error should mention path is not a file"
        );
    }

    #[test]
    fn resolve_body_handles_empty_file() {
        let (_dir, file) = temp_file("");

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some(""));
    }

    #[test]
    fn resolve_body_handles_file_with_only_newlines() {
        let (_dir, file) = temp_file("\n\n\n");

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some(""));
    }
}
