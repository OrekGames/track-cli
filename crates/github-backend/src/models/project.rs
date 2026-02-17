use serde::{Deserialize, Serialize};

use super::issue::GitHubUser;

/// GitHub repository (used as "project")
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubRepo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub owner: GitHubUser,
}
