use serde::{Deserialize, Serialize};

use super::user::JiraUser;

/// Jira comment
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraComment {
    /// Comment ID
    pub id: String,
    /// Self URL
    #[serde(rename = "self")]
    pub self_url: Option<String>,
    /// Comment body in ADF (Atlassian Document Format) or plain text
    pub body: serde_json::Value,
    /// Comment author
    pub author: Option<JiraUser>,
    /// Creation timestamp
    pub created: Option<String>,
    /// Last update timestamp
    pub updated: Option<String>,
}

/// Response from listing comments
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraCommentsResponse {
    pub comments: Vec<JiraComment>,
    #[serde(default)]
    pub start_at: usize,
    #[serde(default)]
    pub max_results: usize,
    #[serde(default)]
    pub total: usize,
}

/// Request to create a comment
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJiraComment {
    pub body: serde_json::Value,
}

impl CreateJiraComment {
    /// Create a simple plain text comment using ADF format
    pub fn from_text(text: &str) -> Self {
        Self {
            body: serde_json::json!({
                "type": "doc",
                "version": 1,
                "content": [
                    {
                        "type": "paragraph",
                        "content": [
                            {
                                "type": "text",
                                "text": text
                            }
                        ]
                    }
                ]
            }),
        }
    }
}
