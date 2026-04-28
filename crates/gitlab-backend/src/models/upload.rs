use serde::{Deserialize, Serialize};

/// GitLab project upload response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabUpload {
    pub alt: String,
    pub url: String,
    #[serde(default)]
    pub full_path: Option<String>,
    pub markdown: String,
}
