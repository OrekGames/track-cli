use serde::{Deserialize, Serialize};

/// GitLab label
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabLabel {
    pub id: u64,
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}
