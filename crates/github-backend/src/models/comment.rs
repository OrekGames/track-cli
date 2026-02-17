use serde::{Deserialize, Serialize};

use super::issue::GitHubUser;

/// GitHub issue comment
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubComment {
    pub id: u64,
    pub body: String,
    pub user: Option<GitHubUser>,
    pub created_at: String,
    pub updated_at: String,
}

/// Request body for creating a comment
#[derive(Debug, Clone, Serialize)]
pub struct CreateGitHubComment {
    pub body: String,
}
