use thiserror::Error;
use tracker_core::TrackerError;

#[derive(Error, Debug)]
pub enum GitHubError {
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

    #[error("Wiki error: {0}")]
    Wiki(String),
}

pub type Result<T> = std::result::Result<T, GitHubError>;

impl From<GitHubError> for TrackerError {
    fn from(err: GitHubError) -> Self {
        match err {
            GitHubError::Http(e) => TrackerError::Http(e.to_string()),
            GitHubError::Parse(e) => TrackerError::Parse(e.to_string()),
            GitHubError::Io(e) => TrackerError::Io(e.to_string()),
            GitHubError::IssueNotFound(id) => TrackerError::IssueNotFound(id),
            GitHubError::ProjectNotFound(id) => TrackerError::ProjectNotFound(id),
            GitHubError::Unauthorized => TrackerError::Unauthorized,
            GitHubError::RateLimited => TrackerError::Api {
                status: 429,
                message: "GitHub API rate limit exceeded".to_string(),
            },
            GitHubError::Api { status, message } => TrackerError::Api { status, message },
            GitHubError::Wiki(msg) => TrackerError::Api {
                status: 500,
                message: format!("Wiki error: {}", msg),
            },
        }
    }
}
