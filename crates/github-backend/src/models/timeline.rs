use serde::{Deserialize, Serialize};

use super::issue::GitHubUser;

/// A single GitHub issue timeline event.
///
/// The GitHub "List timeline events" API returns a heterogeneous stream of
/// events discriminated by an `event` string field. We model only the
/// transition events that map cleanly onto an [`tracker_core::IssueHistoryEvent`]
/// and route everything else (commented, referenced, cross-referenced, etc.)
/// to [`GitHubTimelineEvent::Other`].
///
/// `#[serde(other)]` is **not** supported on internally tagged enums
/// (`#[serde(tag = ...)]`), so the discriminator is matched manually in
/// [`from_value`] rather than relying on serde's tagged dispatch.
#[derive(Debug, Clone)]
pub enum GitHubTimelineEvent {
    Closed {
        created_at: Option<String>,
        actor: Option<GitHubUser>,
    },
    Reopened {
        created_at: Option<String>,
        actor: Option<GitHubUser>,
    },
    Assigned {
        created_at: Option<String>,
        actor: Option<GitHubUser>,
        assignee: Option<GitHubUser>,
    },
    Unassigned {
        created_at: Option<String>,
        actor: Option<GitHubUser>,
        assignee: Option<GitHubUser>,
    },
    Labeled {
        created_at: Option<String>,
        actor: Option<GitHubUser>,
        label: Option<TimelineLabel>,
    },
    Unlabeled {
        created_at: Option<String>,
        actor: Option<GitHubUser>,
        label: Option<TimelineLabel>,
    },
    Renamed {
        created_at: Option<String>,
        actor: Option<GitHubUser>,
        rename: Option<GitHubRename>,
    },
    Milestoned {
        created_at: Option<String>,
        actor: Option<GitHubUser>,
        milestone: Option<TimelineMilestone>,
    },
    Demilestoned {
        created_at: Option<String>,
        actor: Option<GitHubUser>,
        milestone: Option<TimelineMilestone>,
    },
    /// Any event type we don't translate (commented, referenced, ...).
    Other,
}

/// The `rename` payload on a `renamed` timeline event.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubRename {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// The `label` payload on a `labeled`/`unlabeled` timeline event.
///
/// The timeline API sends a narrower object than the full issue-API label
/// (no `id`/`description`), so this is a dedicated subset type rather than
/// reusing [`super::label::GitHubLabel`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimelineLabel {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

/// The `milestone` payload on a `milestoned`/`demilestoned` timeline event.
///
/// The timeline API sends a narrower object than the full issue-API
/// milestone (no `id`/`number`), so this is a dedicated subset type rather
/// than reusing [`super::issue::GitHubMilestone`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimelineMilestone {
    pub title: String,
}

/// Flattened representation of the raw timeline event JSON, used as a
/// deserialization target before we dispatch on the `event` discriminator.
///
/// All fields are optional because each concrete event type only populates a
/// subset; unknown event types simply leave them all `None`.
#[derive(Debug, Clone, Deserialize)]
struct RawTimelineEvent {
    event: Option<String>,
    created_at: Option<String>,
    actor: Option<GitHubUser>,
    assignee: Option<GitHubUser>,
    label: Option<TimelineLabel>,
    rename: Option<GitHubRename>,
    milestone: Option<TimelineMilestone>,
}

impl GitHubTimelineEvent {
    /// Map a raw timeline JSON value onto a typed event, routing unrecognized
    /// `event` discriminators to [`GitHubTimelineEvent::Other`].
    pub fn from_value(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        let raw: RawTimelineEvent = serde_json::from_value(value)?;
        let RawTimelineEvent {
            event,
            created_at,
            actor,
            assignee,
            label,
            rename,
            milestone,
        } = raw;

        Ok(match event.as_deref() {
            Some("closed") => GitHubTimelineEvent::Closed { created_at, actor },
            Some("reopened") => GitHubTimelineEvent::Reopened { created_at, actor },
            Some("assigned") => GitHubTimelineEvent::Assigned {
                created_at,
                actor,
                assignee,
            },
            Some("unassigned") => GitHubTimelineEvent::Unassigned {
                created_at,
                actor,
                assignee,
            },
            Some("labeled") => GitHubTimelineEvent::Labeled {
                created_at,
                actor,
                label,
            },
            Some("unlabeled") => GitHubTimelineEvent::Unlabeled {
                created_at,
                actor,
                label,
            },
            Some("renamed") => GitHubTimelineEvent::Renamed {
                created_at,
                actor,
                rename,
            },
            Some("milestoned") => GitHubTimelineEvent::Milestoned {
                created_at,
                actor,
                milestone,
            },
            Some("demilestoned") => GitHubTimelineEvent::Demilestoned {
                created_at,
                actor,
                milestone,
            },
            _ => GitHubTimelineEvent::Other,
        })
    }
}
