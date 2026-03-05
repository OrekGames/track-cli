pub mod article;
pub mod bundle;
pub mod cache;
pub mod config;
pub mod context;
pub mod eval;
pub mod field;
pub mod init;
pub mod issue;
pub mod open;
pub mod project;
pub mod tags;

use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;

/// Resolve text content from an inline string or a file path.
///
/// If `body_file` is `Some`, reads from the given path (use `"-"` for stdin).
/// Otherwise returns `inline` as-is. Trims a single trailing newline from
/// file/stdin content so that the editor-appended newline isn't sent to the API.
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
            std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read '{}'", path.display()))?
        };
        Ok(Some(content.trim_end_matches('\n').to_string()))
    } else {
        Ok(inline.map(String::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        let dir = std::env::temp_dir().join("track-test-resolve-body");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("desc.md");
        std::fs::write(&file, "## Title\n\nBody text\n").unwrap();

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("## Title\n\nBody text"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_body_trims_trailing_newline_from_file() {
        let dir = std::env::temp_dir().join("track-test-resolve-trim");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("body.txt");
        std::fs::write(&file, "content\n").unwrap();

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("content"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_body_trims_multiple_trailing_newlines() {
        let dir = std::env::temp_dir().join("track-test-resolve-multi-nl");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("body.txt");
        std::fs::write(&file, "content\n\n\n").unwrap();

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("content"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_body_preserves_internal_newlines() {
        let dir = std::env::temp_dir().join("track-test-resolve-internal");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("body.txt");
        std::fs::write(&file, "line1\nline2\nline3\n").unwrap();

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("line1\nline2\nline3"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_body_file_takes_precedence_over_inline() {
        let dir = std::env::temp_dir().join("track-test-resolve-precedence");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("body.txt");
        std::fs::write(&file, "from file\n").unwrap();

        let result = resolve_body(Some("from inline"), Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some("from file"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_body_errors_on_missing_file() {
        let path = PathBuf::from("/tmp/track-test-nonexistent-file-xyz.md");
        let result = resolve_body(None, Some(&path));
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Failed to read"),
            "error should mention file read failure"
        );
    }

    #[test]
    fn resolve_body_handles_empty_file() {
        let dir = std::env::temp_dir().join("track-test-resolve-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("empty.txt");
        std::fs::write(&file, "").unwrap();

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some(""));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_body_handles_file_with_only_newlines() {
        let dir = std::env::temp_dir().join("track-test-resolve-only-nl");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("newlines.txt");
        std::fs::write(&file, "\n\n\n").unwrap();

        let result = resolve_body(None, Some(&file)).unwrap();
        assert_eq!(result.as_deref(), Some(""));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
