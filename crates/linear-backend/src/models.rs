use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct GraphQlRequest<'a, V: Serialize> {
    pub query: &'a str,
    pub variables: V,
}

#[derive(Debug, Deserialize)]
pub struct GraphQlResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GraphQlError>>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQlError {
    pub message: String,
    pub extensions: Option<GraphQlErrorExtensions>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQlErrorExtensions {
    pub code: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearPageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(bound(deserialize = "T: Deserialize<'de>", serialize = "T: Serialize"))]
pub struct LinearConnection<T> {
    #[serde(default)]
    pub nodes: Vec<T>,
    pub page_info: LinearPageInfo,
    #[serde(default)]
    pub total_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinearTeam {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearUser {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinearWorkflowState {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub state_type: String,
    #[serde(default)]
    pub position: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinearIssueLabel {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearProject {
    pub id: String,
    pub name: String,
    pub slug_id: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub priority: i64,
    pub priority_label: Option<String>,
    pub url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub canceled_at: Option<DateTime<Utc>>,
    pub team: LinearTeam,
    pub state: Option<LinearWorkflowState>,
    pub assignee: Option<LinearUser>,
    pub project: Option<LinearProject>,
    pub parent: Option<LinearIssueRef>,
    pub labels: LinearConnection<LinearIssueLabel>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinearIssueRef {
    pub id: String,
    pub identifier: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearComment {
    pub id: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub user: Option<LinearUser>,
}

/// A workflow state reference as it appears in an issue-history transition.
///
/// History nodes only need the state's display `name`; the full
/// [`LinearWorkflowState`] is overkill (and its `type`/`position` fields are not
/// requested on the history query).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinearHistoryState {
    pub name: String,
}

/// One entry in an issue's change history (`Issue.history` connection).
///
/// A single node may carry several independent transitions (state, assignee,
/// priority, title, ...); each populated transition is decomposed into its own
/// [`tracker_core::IssueHistoryEvent`] by the converter, all sharing this
/// node's `created_at` and `actor`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueHistory {
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    pub actor: Option<LinearUser>,
    pub from_state: Option<LinearHistoryState>,
    pub to_state: Option<LinearHistoryState>,
    pub from_assignee: Option<LinearUser>,
    pub to_assignee: Option<LinearUser>,
    pub from_priority: Option<i64>,
    pub to_priority: Option<i64>,
    pub from_title: Option<String>,
    pub to_title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueRelation {
    pub id: String,
    #[serde(rename = "type")]
    pub relation_type: String,
    pub issue: LinearIssueRef,
    pub related_issue: LinearIssueRef,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueCreateInput {
    pub title: String,
    pub team_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueUpdateInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssuePayload {
    pub success: bool,
    pub issue: Option<LinearIssue>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearCommentPayload {
    pub success: bool,
    pub comment: Option<LinearComment>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueLabelPayload {
    pub success: bool,
    pub issue_label: Option<LinearIssueLabel>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueRelationPayload {
    pub success: bool,
    pub issue_relation: Option<LinearIssueRelation>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LinearDeletePayload {
    pub success: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearCommentCreateInput {
    pub issue_id: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueLabelCreateInput {
    pub team_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueLabelUpdateInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueRelationCreateInput {
    pub issue_id: String,
    pub related_issue_id: String,
    #[serde(rename = "type")]
    pub relation_type: String,
}
