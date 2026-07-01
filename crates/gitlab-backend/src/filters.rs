//! Unified GitLab issue-list filter representation.

/// All GitLab issue-list filter dimensions the CLI query language can express.
/// Both list/search and count go through this so a filter can never be
/// supported on one code path and silently dropped on another.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitLabIssueFilters {
    pub search: String,
    pub state: Option<String>,
    pub labels: Option<String>,
    pub assignee_username: Option<String>,
    pub author_username: Option<String>,
    pub milestone: Option<String>,
    pub order_by: Option<String>,
    pub sort: Option<String>,
}
