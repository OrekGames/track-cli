use serde::{Deserialize, Serialize};

use super::issue::GitLabUser;

/// GitLab resource state event
/// (`GET /projects/:id/issues/:iid/resource_state_events`).
///
/// `state` is one of `opened`/`closed`/`reopened`/... The `user` is absent for
/// some system-generated events.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabStateEvent {
    pub id: u64,
    pub user: Option<GitLabUser>,
    pub created_at: Option<String>,
    pub state: Option<String>,
}

/// Label reference embedded in a [`GitLabLabelEvent`].
///
/// Only the name is needed for history rendering.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabEventLabel {
    #[serde(default)]
    pub name: String,
}

/// GitLab resource label event
/// (`GET /projects/:id/issues/:iid/resource_label_events`).
///
/// `action` is `"add"` or `"remove"`. `label` may be `null` if the referenced
/// label has since been deleted.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabLabelEvent {
    pub id: u64,
    pub user: Option<GitLabUser>,
    pub created_at: Option<String>,
    pub action: Option<String>,
    pub label: Option<GitLabEventLabel>,
}

/// Milestone reference embedded in a [`GitLabMilestoneEvent`].
///
/// Only the title is needed for history rendering.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabEventMilestone {
    #[serde(default)]
    pub title: String,
}

/// GitLab resource milestone event
/// (`GET /projects/:id/issues/:iid/resource_milestone_events`).
///
/// `action` is `"add"` or `"remove"`. `milestone` may be `null` for system or
/// removed milestones.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitLabMilestoneEvent {
    pub id: u64,
    pub user: Option<GitLabUser>,
    pub created_at: Option<String>,
    pub action: Option<String>,
    pub milestone: Option<GitLabEventMilestone>,
}
