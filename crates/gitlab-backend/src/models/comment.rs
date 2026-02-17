use serde::{Deserialize, Serialize};

use super::issue::GitLabUser;

/// GitLab note (comment)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabNote {
    pub id: u64,
    pub body: String,
    pub author: Option<GitLabUser>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub system: bool,
}

/// Request to create a note
#[derive(Debug, Clone, Serialize)]
pub struct CreateGitLabNote {
    pub body: String,
}
