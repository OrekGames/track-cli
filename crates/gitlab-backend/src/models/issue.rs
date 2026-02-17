use serde::{Deserialize, Serialize};

/// GitLab user reference (used in assignee, author, etc.)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabUser {
    pub id: u64,
    pub username: String,
    #[serde(default)]
    pub name: String,
}

/// GitLab milestone reference
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabMilestone {
    pub id: u64,
    pub iid: u64,
    pub title: String,
}

/// GitLab issue
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabIssue {
    pub id: u64,
    pub iid: u64,
    pub project_id: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<String>,
    pub assignee: Option<GitLabUser>,
    #[serde(default)]
    pub assignees: Vec<GitLabUser>,
    pub milestone: Option<GitLabMilestone>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub closed_at: Option<String>,
    pub author: Option<GitLabUser>,
    pub web_url: Option<String>,
}

/// GitLab linked issue (response from GET /projects/:id/issues/:iid/links).
///
/// The GET endpoint returns a flat array of issue objects, each augmented with
/// `issue_link_id` and `link_type` metadata fields.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabIssueLink {
    pub id: u64,
    pub iid: u64,
    pub title: String,
    pub issue_link_id: u64,
    pub link_type: String,
}

/// Request to create a GitLab issue
#[derive(Debug, Clone, Serialize)]
pub struct CreateGitLabIssue {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_id: Option<u64>,
}

/// Request to update a GitLab issue
#[derive(Debug, Clone, Serialize, Default)]
pub struct UpdateGitLabIssue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee_ids: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_id: Option<u64>,
}

/// Request to create an issue link
#[derive(Debug, Clone, Serialize)]
pub struct CreateGitLabIssueLink {
    pub target_project_id: String,
    pub target_issue_iid: u64,
    pub link_type: String,
}
