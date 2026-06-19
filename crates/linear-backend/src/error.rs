use thiserror::Error;
use tracker_core::TrackerError;

#[derive(Error, Debug)]
pub enum LinearError {
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

    #[error("Rate limited")]
    RateLimited,

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Pagination stalled: {0}")]
    PaginationStalled(String),
}

pub type Result<T> = std::result::Result<T, LinearError>;

impl From<LinearError> for TrackerError {
    fn from(err: LinearError) -> Self {
        match err {
            LinearError::Http(e) => TrackerError::Http(e.to_string()),
            LinearError::Parse(e) => TrackerError::Parse(e.to_string()),
            LinearError::Io(e) => TrackerError::Io(e.to_string()),
            LinearError::IssueNotFound(id) => TrackerError::IssueNotFound(id),
            LinearError::ProjectNotFound(id) => TrackerError::ProjectNotFound(id),
            LinearError::Unauthorized => TrackerError::Unauthorized,
            LinearError::RateLimited => TrackerError::Api {
                status: 429,
                message: "Linear API rate limit exceeded".to_string(),
            },
            LinearError::Api { status, message } => TrackerError::Api { status, message },
            LinearError::PaginationStalled(msg) => TrackerError::PaginationStalled(msg),
        }
    }
}
