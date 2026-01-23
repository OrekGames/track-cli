use thiserror::Error;
use tracker_core::TrackerError;

#[derive(Error, Debug)]
pub enum YouTrackError {
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

pub type Result<T> = std::result::Result<T, YouTrackError>;

impl From<YouTrackError> for TrackerError {
    fn from(err: YouTrackError) -> Self {
        match err {
            YouTrackError::Http(e) => TrackerError::Http(e.to_string()),
            YouTrackError::Parse(e) => TrackerError::Parse(e.to_string()),
            YouTrackError::Io(e) => TrackerError::Io(e.to_string()),
            YouTrackError::IssueNotFound(id) => TrackerError::IssueNotFound(id),
            YouTrackError::ProjectNotFound(id) => TrackerError::ProjectNotFound(id),
            YouTrackError::Unauthorized => TrackerError::Unauthorized,
            YouTrackError::Api { status, message } => TrackerError::Api { status, message },
        }
    }
}
