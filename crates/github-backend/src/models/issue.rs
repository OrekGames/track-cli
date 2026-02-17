use serde::{Deserialize, Serialize};

use super::label::GitHubLabel;

/// GitHub user (minimal representation)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubUser {
    pub login: String,
    pub id: u64,
}

/// GitHub milestone
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubMilestone {
    pub id: u64,
    pub number: u64,
    pub title: String,
}

/// GitHub pull request indicator (presence means the issue is actually a PR)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubPullRequest {
    pub url: Option<String>,
}

/// GitHub issue
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubIssue {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<GitHubLabel>,
    pub assignee: Option<GitHubUser>,
    #[serde(default)]
    pub assignees: Vec<GitHubUser>,
    pub milestone: Option<GitHubMilestone>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub user: Option<GitHubUser>,
    /// If present (non-null), this "issue" is actually a pull request
    pub pull_request: Option<GitHubPullRequest>,
}

impl GitHubIssue {
    /// Returns true if this is actually a pull request, not an issue
    pub fn is_pull_request(&self) -> bool {
        self.pull_request.is_some()
    }
}

/// GitHub search result
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubSearchResult {
    pub total_count: u64,
    pub incomplete_results: bool,
    pub items: Vec<GitHubIssue>,
}

/// Request body for creating a GitHub issue
#[derive(Debug, Clone, Serialize)]
pub struct CreateGitHubIssue {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone: Option<u64>,
}

/// Request body for updating a GitHub issue
#[derive(Debug, Clone, Serialize)]
pub struct UpdateGitHubIssue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone: Option<u64>,
}
