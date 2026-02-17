use serde::{Deserialize, Serialize};

/// GitLab label
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabLabel {
    pub id: u64,
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

/// Create a new GitLab label
#[derive(Debug, Clone, Serialize)]
pub struct CreateGitLabLabel {
    pub name: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Update a GitLab label
#[derive(Debug, Clone, Serialize)]
pub struct UpdateGitLabLabel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
