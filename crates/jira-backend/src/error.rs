use thiserror::Error;
use tracker_core::TrackerError;

#[derive(Error, Debug)]
pub enum JiraError {
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

pub type Result<T> = std::result::Result<T, JiraError>;

impl From<JiraError> for TrackerError {
    fn from(err: JiraError) -> Self {
        match err {
            JiraError::Http(e) => TrackerError::Http(e.to_string()),
            JiraError::Parse(e) => TrackerError::Parse(e.to_string()),
            JiraError::Io(e) => TrackerError::Io(e.to_string()),
            JiraError::IssueNotFound(id) => TrackerError::IssueNotFound(id),
            JiraError::ProjectNotFound(id) => TrackerError::ProjectNotFound(id),
            JiraError::Unauthorized => TrackerError::Unauthorized,
            JiraError::Api { status, message } => TrackerError::Api { status, message },
        }
    }
}
