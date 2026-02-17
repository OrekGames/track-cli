use thiserror::Error;
use tracker_core::TrackerError;

#[derive(Error, Debug)]
pub enum GitLabError {
    #[error("HTTP error: {0}")]
    Http(#[from] ureq::Error),

    #[error("JSON parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Issue not found: {0}")]
    IssueNotFound(String),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Authentication failed")]
    Unauthorized,

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },
}

pub type Result<T> = std::result::Result<T, GitLabError>;

impl From<GitLabError> for TrackerError {
    fn from(err: GitLabError) -> Self {
        match err {
            GitLabError::Http(e) => TrackerError::Http(e.to_string()),
            GitLabError::Parse(e) => TrackerError::Parse(e.to_string()),
            GitLabError::Io(e) => TrackerError::Io(e.to_string()),
            GitLabError::IssueNotFound(id) => TrackerError::IssueNotFound(id),
            GitLabError::ProjectNotFound(id) => TrackerError::ProjectNotFound(id),
            GitLabError::Unauthorized => TrackerError::Unauthorized,
            GitLabError::Api { status, message } => TrackerError::Api { status, message },
        }
    }
}
