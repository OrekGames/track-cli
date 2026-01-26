use serde::{Deserialize, Serialize};

/// Jira user representation
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraUser {
    /// User account ID (Jira Cloud uses account IDs, not usernames)
    pub account_id: Option<String>,
    /// Display name
    pub display_name: Option<String>,
    /// Email address (may not be visible depending on privacy settings)
    pub email_address: Option<String>,
    /// Whether the user is active
    #[serde(default)]
    pub active: bool,
    /// Self URL
    #[serde(rename = "self")]
    pub self_url: Option<String>,
}
