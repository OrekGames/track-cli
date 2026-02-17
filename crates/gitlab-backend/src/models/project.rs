use serde::{Deserialize, Serialize};

/// GitLab project
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabProject {
    pub id: u64,
    pub name: String,
    pub name_with_namespace: Option<String>,
    pub path: Option<String>,
    pub path_with_namespace: Option<String>,
    pub description: Option<String>,
    pub web_url: Option<String>,
}
