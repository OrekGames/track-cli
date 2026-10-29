use thiserror::Error;

/// Common errors for all issue tracker backends
#[derive(Error, Debug)]
pub enum TrackerError {
    #[error("Authentication failed")]
    Unauthorized,

    #[error("Issue not found: {0}")]
    IssueNotFound(String),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// The backend does not implement this operation.
    #[error("{0}")]
    Unsupported(String),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Pagination stalled: {0}")]
    PaginationStalled(String),

    #[error("IO error: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, TrackerError>;

// Payloads stay Strings and no source is kept: anyhow's `{:#}` would otherwise
// print each cause twice, and tracker-mock and the doctor tests build these
// variants from strings.

/// A request that could not be sent, or whose response could not be read:
/// connection, TLS, timeout, body limit, or a request ureq could not build.
/// ureq's text is kept verbatim; the CLI looks in it for certificate problems
/// ("UnknownIssuer"). Decode response bodies with [`crate::http::read_json`],
/// which reports a body that is not the expected JSON as [`TrackerError::Parse`].
impl From<ureq::Error> for TrackerError {
    fn from(err: ureq::Error) -> Self {
        TrackerError::Http(err.to_string())
    }
}

/// Data that is not the expected JSON.
impl From<serde_json::Error> for TrackerError {
    fn from(err: serde_json::Error) -> Self {
        TrackerError::Parse(err.to_string())
    }
}

/// A local file that cannot be read, such as an attachment to upload.
impl From<std::io::Error> for TrackerError {
    fn from(err: std::io::Error) -> Self {
        TrackerError::Io(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_ureq_error_keeps_ureq_text() {
        let err = ureq::Error::Io(std::io::Error::other("boom"));
        let text = err.to_string();

        let err = TrackerError::from(err);

        assert_eq!(err.to_string(), format!("HTTP error: {text}"));
        let TrackerError::Http(message) = err else {
            panic!("expected Http, got {err:?}");
        };
        assert_eq!(message, text);
    }

    #[test]
    fn from_serde_json_and_io_errors() {
        let parse = TrackerError::from(serde_json::from_str::<u8>("x").unwrap_err());
        assert!(matches!(parse, TrackerError::Parse(_)), "{parse:?}");

        let io = TrackerError::from(std::io::Error::other("e"));
        assert!(
            matches!(io, TrackerError::Io(ref message) if message == "e"),
            "{io:?}"
        );
    }
}
