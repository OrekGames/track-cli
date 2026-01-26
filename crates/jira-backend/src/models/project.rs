use serde::{Deserialize, Serialize};

/// Jira project
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraProject {
    /// Internal ID
    pub id: String,
    /// Project key (e.g., "PROJ")
    pub key: String,
    /// Project name
    pub name: String,
    /// Self URL
    #[serde(rename = "self")]
    pub self_url: Option<String>,
    /// Project type key (e.g., "software", "business")
    pub project_type_key: Option<String>,
    /// Project description
    pub description: Option<String>,
}

/// Project reference (used in issue responses)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraProjectRef {
    /// Internal ID
    pub id: String,
    /// Project key
    pub key: String,
    /// Project name
    pub name: Option<String>,
    /// Self URL
    #[serde(rename = "self")]
    pub self_url: Option<String>,
}

/// Request to create a new project
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJiraProject {
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub project_type_key: String,
    pub lead_account_id: String,
}
