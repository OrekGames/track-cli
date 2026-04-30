use std::collections::HashMap;

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
    /// Attachments on this issue.
    #[serde(default)]
    pub attachment: Vec<JiraAttachment>,
    /// Extra/custom fields not captured by the named fields above.
    /// Keys are field IDs like "customfield_10016".
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Comments container in issue response
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraCommentsContainer {
    pub comments: Vec<JiraComment>,
    #[serde(default)]
    pub total: usize,
}

/// Jira issue attachment.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraAttachment {
    pub id: String,
    pub filename: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub author: Option<JiraUser>,
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

/// Search result response from `/search/jql` endpoint.
///
/// The new Jira Cloud search endpoint returns `isLast` instead of `total`.
/// It does not provide a total count of matching issues.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSearchResult {
    /// Issues in this page
    pub issues: Vec<JiraIssue>,
    /// Whether this is the last page of results
    #[serde(default)]
    pub is_last: bool,
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
    /// Arbitrary custom fields (e.g., "customfield_10016": 5)
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentId>,
    /// Arbitrary custom fields (e.g., "customfield_10016": 5)
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
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

/// Convert a markdown string to an ADF document.
///
/// Parses CommonMark + GFM and maps constructs to ADF node types.
pub fn markdown_to_adf(text: &str) -> serde_json::Value {
    crate::markdown::adf::markdown_to_adf(text)
}

/// Extract plain text from ADF document
pub fn adf_to_text(adf: &serde_json::Value) -> String {
    crate::markdown::adf::adf_to_text(adf)
}
