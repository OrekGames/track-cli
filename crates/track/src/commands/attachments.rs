#![allow(dead_code)]

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use tracker_core::{AttachmentUpload, AttachmentUploadFile};

pub(crate) fn build_attachment_upload(
    paths: &[PathBuf],
    name: Option<&str>,
    mime_type: Option<&str>,
    comment: Option<&str>,
    silent: bool,
    minor_edit: bool,
) -> Result<AttachmentUpload> {
    if paths.is_empty() {
        return Err(anyhow!("At least one attachment path is required"));
    }

    if paths.len() != 1 && name.is_some() {
        return Err(anyhow!(
            "--name can only be used when uploading exactly one file"
        ));
    }

    if paths.len() != 1 && mime_type.is_some() {
        return Err(anyhow!(
            "--mime-type can only be used when uploading exactly one file"
        ));
    }

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        validate_attachment_path(path)?;
        files.push(AttachmentUploadFile {
            path: path.clone(),
            name: (paths.len() == 1)
                .then(|| name.map(str::to_string))
                .flatten(),
            mime_type: (paths.len() == 1)
                .then(|| mime_type.map(str::to_string))
                .flatten(),
        });
    }

    Ok(AttachmentUpload {
        files,
        comment: comment.map(str::to_string),
        silent,
        minor_edit,
    })
}

fn validate_attachment_path(path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path)
        .map_err(|err| anyhow!("Failed to read attachment '{}': {}", path.display(), err))?;

    if !metadata.is_file() {
        return Err(anyhow!(
            "Attachment path '{}' is not a file",
            path.display()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp_file(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("track-attachment-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(name);
        std::fs::write(&file, b"test").unwrap();
        file
    }

    #[test]
    fn builds_upload_for_single_file_with_overrides() {
        // Arrange
        let file = write_temp_file("single.bin");

        // Act
        let upload = build_attachment_upload(
            std::slice::from_ref(&file),
            Some("custom.bin"),
            Some("application/octet-stream"),
            Some("upload note"),
            true,
            false,
        )
        .unwrap();

        // Assert
        assert_eq!(upload.files.len(), 1);
        assert_eq!(upload.files[0].path, file);
        assert_eq!(upload.files[0].name.as_deref(), Some("custom.bin"));
        assert_eq!(
            upload.files[0].mime_type.as_deref(),
            Some("application/octet-stream")
        );
        assert_eq!(upload.comment.as_deref(), Some("upload note"));
        assert!(upload.silent);
        assert!(!upload.minor_edit);
    }

    #[test]
    fn rejects_name_with_multiple_files() {
        // Arrange
        let first = write_temp_file("first.txt");
        let second = write_temp_file("second.txt");

        // Act
        let result = build_attachment_upload(
            &[first, second],
            Some("custom.txt"),
            None,
            None,
            false,
            false,
        );

        // Assert
        assert!(result.is_err());
    }

    #[test]
    fn rejects_mime_type_with_multiple_files() {
        // Arrange
        let first = write_temp_file("mime-first.txt");
        let second = write_temp_file("mime-second.txt");

        // Act
        let result = build_attachment_upload(
            &[first, second],
            None,
            Some("text/plain"),
            None,
            false,
            false,
        );

        // Assert
        assert!(result.is_err());
    }

    #[test]
    fn rejects_missing_file() {
        // Arrange
        let missing = std::env::temp_dir().join("track-missing-attachment-file");

        // Act
        let result = build_attachment_upload(&[missing], None, None, None, false, false);

        // Assert
        assert!(result.is_err());
    }
}
