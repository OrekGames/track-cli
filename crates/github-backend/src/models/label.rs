use serde::{Deserialize, Serialize};

/// GitHub label
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubLabel {
    pub id: u64,
    pub name: String,
    /// Color hex string WITHOUT `#` prefix (e.g., "fc2929")
    pub color: String,
    pub description: Option<String>,
}

/// Create a new GitHub label
#[derive(Debug, Clone, Serialize)]
pub struct CreateGitHubLabel {
    pub name: String,
    /// Color hex string WITHOUT `#` prefix (e.g., "fc2929")
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Update a GitHub label
#[derive(Debug, Clone, Serialize)]
pub struct UpdateGitHubLabel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
