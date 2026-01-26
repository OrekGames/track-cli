use serde::{Deserialize, Serialize};

use super::comment::JiraComment;
use super::project::JiraProjectRef;
use super::user::JiraUser;

/// Jira issue
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssue {
    /// Internal numeric ID
    pub id: String,
    /// Issue key (e.g., "PROJ-123")
    pub key: String,
    /// Self URL
    #[serde(rename = "self")]
    pub self_url: Option<String>,
    /// Issue fields
    pub fields: JiraIssueFields,
}

/// Issue fields container
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueFields {
    /// Issue summary/title
    pub summary: String,
    /// Issue description in ADF format
    pub description: Option<serde_json::Value>,
    /// Issue status
    pub status: JiraStatus,
    /// Issue priority
    pub priority: Option<JiraPriority>,
    /// Issue type
    pub issuetype: JiraIssueType,
    /// Project reference
    pub project: JiraProjectRef,
    /// Assignee
    pub assignee: Option<JiraUser>,
    /// Reporter
    pub reporter: Option<JiraUser>,
    /// Labels (equivalent to tags)
    #[serde(default)]
    pub labels: Vec<String>,
    /// Creation timestamp
    pub created: Option<String>,
    /// Last update timestamp
    pub updated: Option<String>,
    /// Subtasks
    #[serde(default)]
    pub subtasks: Vec<JiraIssueRef>,
    /// Parent issue (if this is a subtask)
    pub parent: Option<JiraIssueRef>,
    /// Issue links
    #[serde(default)]
    pub issuelinks: Vec<JiraIssueLink>,
    /// Comments (only included when expanded)
    pub comment: Option<JiraCommentsContainer>,
}

/// Comments container in issue response
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraCommentsContainer {
    pub comments: Vec<JiraComment>,
    #[serde(default)]
    pub total: usize,
}

/// Issue reference (used in subtasks, parent, links)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueRef {
    /// Internal ID
    pub id: String,
    /// Issue key
    pub key: String,
    /// Self URL
    #[serde(rename = "self")]
    pub self_url: Option<String>,
    /// Summary (sometimes included)
    pub fields: Option<JiraIssueRefFields>,
}

/// Minimal fields in issue reference
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueRefFields {
    pub summary: Option<String>,
    pub status: Option<JiraStatus>,
    pub priority: Option<JiraPriority>,
    pub issuetype: Option<JiraIssueType>,
}

/// Issue status
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraStatus {
    /// Status ID
    pub id: Option<String>,
    /// Status name
    pub name: String,
    /// Status category
    pub status_category: Option<JiraStatusCategory>,
}

/// Status category (used to determine if issue is resolved)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraStatusCategory {
    /// Category key (e.g., "done", "indeterminate", "new")
    pub key: String,
    /// Category name
    pub name: Option<String>,
}

/// Issue priority
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraPriority {
    /// Priority ID
    pub id: Option<String>,
    /// Priority name
    pub name: String,
}

/// Issue type
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueType {
    /// Type ID
    pub id: Option<String>,
    /// Type name
    pub name: String,
    /// Whether this is a subtask type
    #[serde(default)]
    pub subtask: bool,
}

/// Issue link
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueLink {
    /// Link ID
    pub id: Option<String>,
    /// Link type
    #[serde(rename = "type")]
    pub link_type: JiraIssueLinkType,
    /// Inward issue (if this link points inward)
    pub inward_issue: Option<JiraIssueRef>,
    /// Outward issue (if this link points outward)
    pub outward_issue: Option<JiraIssueRef>,
}

/// Issue link type
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueLinkType {
    /// Type ID
    pub id: Option<String>,
    /// Type name
    pub name: String,
    /// Inward description (e.g., "is blocked by")
    pub inward: Option<String>,
    /// Outward description (e.g., "blocks")
    pub outward: Option<String>,
}

/// Search result response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSearchResult {
    /// Starting index
    #[serde(default)]
    pub start_at: usize,
    /// Maximum results per page
    #[serde(default)]
    pub max_results: usize,
    /// Total number of results
    #[serde(default)]
    pub total: usize,
    /// Issues in this page
    pub issues: Vec<JiraIssue>,
}

/// Request to create an issue
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJiraIssue {
    pub fields: CreateJiraIssueFields,
}

/// Fields for issue creation
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJiraIssueFields {
    /// Project (key or id)
    pub project: ProjectId,
    /// Summary
    pub summary: String,
    /// Description in ADF format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<serde_json::Value>,
    /// Issue type
    pub issuetype: IssueTypeId,
    /// Priority
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<PriorityId>,
    /// Labels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// Parent issue (for subtasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentId>,
}

/// Project identifier for requests
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectId {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Issue type identifier for requests
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTypeId {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Priority identifier for requests
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorityId {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Parent issue identifier for requests
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentId {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Request to update an issue
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJiraIssue {
    pub fields: UpdateJiraIssueFields,
}

/// Fields for issue update
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJiraIssueFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<PriorityId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

/// Request to create an issue link
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJiraIssueLink {
    #[serde(rename = "type")]
    pub link_type: IssueLinkTypeName,
    pub inward_issue: IssueKeyRef,
    pub outward_issue: IssueKeyRef,
}

/// Issue link type name for requests
#[derive(Debug, Clone, Serialize)]
pub struct IssueLinkTypeName {
    pub name: String,
}

/// Issue key reference for requests
#[derive(Debug, Clone, Serialize)]
pub struct IssueKeyRef {
    pub key: String,
}

/// Request for JQL search
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSearchRequest {
    pub jql: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<Vec<String>>,
}

/// Helper to create text description in ADF format
pub fn text_to_adf(text: &str) -> serde_json::Value {
    serde_json::json!({
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
    })
}

/// Extract plain text from ADF document
pub fn adf_to_text(adf: &serde_json::Value) -> String {
    fn extract_text(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Object(obj) => {
                if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                    return text.to_string();
                }
                if let Some(content) = obj.get("content") {
                    return extract_text(content);
                }
                String::new()
            }
            serde_json::Value::Array(arr) => {
                arr.iter().map(extract_text).collect::<Vec<_>>().join("")
            }
            _ => String::new(),
        }
    }
    extract_text(adf)
}
