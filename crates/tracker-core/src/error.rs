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

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("IO error: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, TrackerError>;
