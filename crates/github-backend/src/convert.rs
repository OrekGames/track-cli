//! Model conversions from GitHub types to tracker-core types

use chrono::{DateTime, Utc};
use tracker_core::{
    Comment, CommentAuthor, CreateIssue, CustomField, CustomFieldUpdate, Issue, IssueHistoryEvent,
    IssueTag, Project, ProjectCustomField, ProjectRef, StateValueInfo, Tag, TagColor, UpdateIssue,
    canonical_field_name,
};

use crate::models::*;

/// Convert a GitHub issue to a tracker-core Issue
///
/// Requires owner/repo context since GitHub issues don't carry full repo info.
pub fn github_issue_to_core(issue: GitHubIssue, owner: &str, repo: &str) -> Issue {
    let id_readable = format!("{}/{}#{}", owner, repo, issue.number);

    let is_resolved = issue.state == "closed";

    let mut custom_fields = Vec::new();

    // Map state as a State custom field
    custom_fields.push(CustomField::State {
        name: "Status".to_string(),
        value: Some(issue.state.clone()),
        is_resolved,
    });

    // Map assignee as a SingleUser custom field
    custom_fields.push(CustomField::SingleUser {
        name: "Assignee".to_string(),
        login: issue.assignee.as_ref().map(|u| u.login.clone()),
        display_name: issue.assignee.as_ref().map(|u| u.login.clone()),
    });

    // Map first label as Type (for compatibility with other backends)
    if let Some(first_label) = issue.labels.first() {
        custom_fields.push(CustomField::SingleEnum {
            name: "Type".to_string(),
            value: Some(first_label.name.clone()),
        });
    }

    // Map milestone as a SingleEnum custom field
    if let Some(ref milestone) = issue.milestone {
        custom_fields.push(CustomField::SingleEnum {
            name: "Milestone".to_string(),
            value: Some(milestone.title.clone()),
        });
    }

    if !issue.assignees.is_empty() {
        let value =
            serde_json::to_value(&issue.assignees).expect("GitHub assignees should serialize");
        if let Some(cf) = github_json_to_custom_field("assignees".to_string(), &value) {
            custom_fields.push(cf);
        }
    }

    if let Some(user) = issue.user.as_ref() {
        let value = serde_json::to_value(user).expect("GitHub user should serialize");
        if let Some(cf) = github_json_to_custom_field("user".to_string(), &value) {
            custom_fields.push(cf);
        }
    }

    // Catch-all: surface every field GitHub returned that no named field
    // claimed (captured by `#[serde(flatten)]` into `issue.extra`). This keeps
    // the projection lossless per the CustomField contract — present values are
    // typed if provable and otherwise round-tripped via Unknown{value}. Keys
    // are sorted for deterministic output. `issue.extra` is borrowed here and
    // is not moved into the `Issue {..}` below, so the borrow is fine.
    let mut keys: Vec<&String> = issue.extra.keys().collect();
    keys.sort();
    for key in keys {
        let value = &issue.extra[key];
        if value.is_null() {
            continue;
        }
        if is_github_noise(key) {
            continue;
        }
        if let Some(cf) = github_json_to_custom_field(key.clone(), value) {
            custom_fields.push(cf);
        }
    }

    let tags: Vec<Tag> = issue
        .labels
        .iter()
        .map(|label| Tag {
            id: label.name.clone(),
            name: label.name.clone(),
        })
        .collect();

    Issue {
        id: issue.number.to_string(),
        id_readable,
        summary: issue.title,
        description: issue.body.filter(|s| !s.is_empty()),
        project: ProjectRef {
            id: format!("{}/{}", owner, repo),
            name: Some(repo.to_string()),
            short_name: Some(format!("{}/{}", owner, repo)),
        },
        custom_fields,
        tags,
        created: parse_github_datetime(&issue.created_at).unwrap_or_else(Utc::now),
        updated: parse_github_datetime(&issue.updated_at).unwrap_or_else(Utc::now),
        resolved: issue.closed_at.as_deref().and_then(parse_github_datetime),
    }
}

/// GitHub issue-payload keys that carry no reporting value and are deliberately
/// dropped from the lossless projection (the per-backend NOISE denylist allowed
/// by the [`CustomField`] contract).
///
/// This covers hypermedia link keys (`url` and any `*_url`) plus a fixed set of
/// API-plumbing / UI-only fields. Everything else GitHub returns is surfaced.
fn is_github_noise(key: &str) -> bool {
    key == "url"
        || key.ends_with("_url")
        || matches!(
            key,
            "node_id"
                | "reactions"
                | "performed_via_github_app"
                | "author_association"
                | "active_lock_reason"
                | "locked"
                | "comments"
                | "score"
                | "draft"
                | "repository"
                | "sub_issues_summary"
                | "timeline_url"
                | "events_url"
        )
}

/// Project a single raw GitHub field value onto the most specific
/// [`CustomField`] variant we can prove, falling back to `Unknown { value }` so
/// the payload round-trips verbatim. Returns `None` only when the value carries
/// nothing to surface (e.g. an empty array).
fn github_json_to_custom_field(name: String, value: &serde_json::Value) -> Option<CustomField> {
    use serde_json::Value;
    match value {
        Value::String(s) => Some(CustomField::Text {
            name,
            value: Some(s.clone()),
        }),
        Value::Number(n) => Some(CustomField::Text {
            name,
            value: Some(format_github_number(n)),
        }),
        Value::Bool(b) => Some(CustomField::Text {
            name,
            value: Some(b.to_string()),
        }),
        Value::Object(map) => {
            // A user-shaped object (`login`) becomes a SingleUser; a named
            // object (`name`/`title`) becomes a SingleEnum; anything richer
            // round-trips as Unknown.
            if let Some(login) = map.get("login").and_then(|v| v.as_str()) {
                let display_name = map
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(login)
                    .to_string();
                Some(CustomField::SingleUser {
                    name,
                    login: Some(login.to_string()),
                    display_name: Some(display_name),
                })
            } else if let Some(label) = map
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| map.get("title").and_then(|v| v.as_str()))
            {
                Some(CustomField::SingleEnum {
                    name,
                    value: Some(label.to_string()),
                })
            } else {
                Some(CustomField::Unknown {
                    name,
                    value: Some(value.clone()),
                })
            }
        }
        Value::Array(arr) => github_json_array_to_custom_field(name, arr, value),
        Value::Null => None,
    }
}

/// Display keys an array item may carry while still counting as a "plain"
/// single-display-key object (vs. a rich object that forces Unknown).
const GITHUB_ARRAY_DISPLAY_KEYS: [&str; 5] = ["name", "title", "value", "login", "display_name"];

/// Project an array value. Per the maintainer HEURISTIC: an array whose items
/// are rich objects (more than one key, or a key outside the display-key set)
/// or nested arrays round-trips whole as `Unknown { value }`; plain string /
/// single-display-key arrays become a `MultiEnum`. `whole` is the original
/// array `Value` so Unknown can carry it verbatim.
fn github_json_array_to_custom_field(
    name: String,
    arr: &[serde_json::Value],
    whole: &serde_json::Value,
) -> Option<CustomField> {
    use serde_json::Value;
    if arr.is_empty() {
        return None;
    }

    // Detect any item that disqualifies the array from MultiEnum.
    let has_rich_item = arr.iter().any(|item| match item {
        Value::Array(_) => true,
        Value::Object(map) => {
            map.len() > 1
                || map
                    .keys()
                    .any(|k| !GITHUB_ARRAY_DISPLAY_KEYS.contains(&k.as_str()))
        }
        _ => false,
    });
    if has_rich_item {
        return Some(CustomField::Unknown {
            name,
            value: Some(whole.clone()),
        });
    }

    // Collect display strings from plain items.
    let values: Vec<String> = arr
        .iter()
        .filter_map(|item| match item {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(format_github_number(n)),
            Value::Bool(b) => Some(b.to_string()),
            Value::Object(map) => GITHUB_ARRAY_DISPLAY_KEYS
                .iter()
                .find_map(|k| map.get(*k).and_then(|v| v.as_str()))
                .map(|s| s.to_string()),
            _ => None,
        })
        .collect();

    if values.is_empty() {
        // Items existed but none yielded a display string — round-trip whole.
        Some(CustomField::Unknown {
            name,
            value: Some(whole.clone()),
        })
    } else {
        Some(CustomField::MultiEnum { name, values })
    }
}

/// Format a JSON number for display, stripping `.0` from whole values. Mirrors
/// the jira backend's `format_number`.
fn format_github_number(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        i.to_string()
    } else if let Some(u) = n.as_u64() {
        u.to_string()
    } else if let Some(f) = n.as_f64() {
        if f.fract() == 0.0 {
            (f as i64).to_string()
        } else {
            f.to_string()
        }
    } else {
        n.to_string()
    }
}

impl From<GitHubComment> for Comment {
    fn from(c: GitHubComment) -> Self {
        Self {
            id: c.id.to_string(),
            text: c.body,
            author: c.user.map(|u| CommentAuthor {
                login: u.login.clone(),
                name: Some(u.login),
            }),
            created: parse_github_datetime(&c.created_at),
        }
    }
}

impl From<GitHubRepo> for Project {
    fn from(r: GitHubRepo) -> Self {
        Self {
            id: r.id.to_string(),
            name: r.name,
            short_name: r.full_name,
            description: r.description,
        }
    }
}

impl From<GitHubLabel> for IssueTag {
    fn from(l: GitHubLabel) -> Self {
        Self {
            id: l.name.clone(),
            name: l.name,
            color: Some(TagColor {
                id: format!("#{}", l.color),
                background: Some(format!("#{}", l.color)),
                foreground: None,
            }),
            issues_count: None,
        }
    }
}

/// Convert a tracker-core CreateIssue to a GitHub CreateGitHubIssue
pub fn create_issue_from_core(issue: &CreateIssue) -> CreateGitHubIssue {
    let assignees: Vec<String> = issue
        .custom_fields
        .iter()
        .filter_map(|cf| match cf {
            CustomFieldUpdate::SingleUser { name, login }
                if name.eq_ignore_ascii_case("assignee") =>
            {
                Some(login.clone())
            }
            _ => None,
        })
        .collect();

    CreateGitHubIssue {
        title: issue.summary.clone(),
        body: issue.description.clone(),
        labels: if issue.tags.is_empty() {
            None
        } else {
            Some(issue.tags.clone())
        },
        assignees: if assignees.is_empty() {
            None
        } else {
            Some(assignees)
        },
        milestone: None,
    }
}

/// Convert a tracker-core UpdateIssue to a GitHub UpdateGitHubIssue
pub fn update_issue_from_core(update: &UpdateIssue) -> UpdateGitHubIssue {
    // Extract state from custom fields (CLI sends "State" or "Stage", backends may use "Status")
    let state = update.custom_fields.iter().find_map(|cf| match cf {
        CustomFieldUpdate::State { name, value }
            if name.eq_ignore_ascii_case("status")
                || name.eq_ignore_ascii_case("state")
                || name.eq_ignore_ascii_case("stage") =>
        {
            let mapped_value = match value.to_lowercase().as_str() {
                "done" | "resolved" | "closed" | "completed" => "closed",
                "open" | "in progress" | "develop" | "reopened" => "open",
                _ => value.as_str(), // Fallback
            };
            Some(mapped_value.to_string())
        }
        _ => None,
    });

    let assignees: Vec<String> = update
        .custom_fields
        .iter()
        .filter_map(|cf| match cf {
            CustomFieldUpdate::SingleUser { name, login }
                if name.eq_ignore_ascii_case("assignee") =>
            {
                Some(login.clone())
            }
            _ => None,
        })
        .collect();

    UpdateGitHubIssue {
        title: update.summary.clone(),
        body: update.description.clone(),
        state,
        labels: if update.tags.is_empty() {
            None
        } else {
            Some(update.tags.clone())
        },
        assignees: if assignees.is_empty() {
            None
        } else {
            Some(assignees)
        },
        milestone: None,
    }
}

/// Convert a simple tracker-core query to GitHub search syntax
pub fn convert_query_to_github(query: &str) -> String {
    let mut parts = Vec::new();
    let mut remaining = query.trim();

    // Handle project: syntax (ignore for GitHub since we're repo-scoped)
    if let Some(rest) = remaining.strip_prefix("project:") {
        let rest = rest.trim_start();
        if let Some(space_pos) = rest.find(' ') {
            remaining = &rest[space_pos..];
        } else {
            remaining = "";
        }
    }

    let tokens: Vec<&str> = remaining.split_whitespace().collect();
    for token in tokens {
        if let Some(state) = token.strip_prefix('#') {
            if state.eq_ignore_ascii_case("unresolved") || state.eq_ignore_ascii_case("open") {
                parts.push("is:open".to_string());
            } else if state.eq_ignore_ascii_case("resolved") || state.eq_ignore_ascii_case("closed")
            {
                parts.push("is:closed".to_string());
            } else {
                parts.push(format!("label:{}", state));
            }
        } else {
            parts.push(token.to_string());
        }
    }

    if parts.is_empty() {
        "is:issue".to_string()
    } else {
        // Always include is:issue to filter out PRs
        parts.push("is:issue".to_string());
        parts.join(" ")
    }
}

/// Get standard GitHub custom fields for a project
pub fn get_standard_custom_fields() -> Vec<ProjectCustomField> {
    vec![
        ProjectCustomField {
            id: "status".to_string(),
            name: "Status".to_string(),
            field_type: "state[1]".to_string(),
            required: true,
            values: vec!["open".to_string(), "closed".to_string()],
            state_values: vec![
                StateValueInfo {
                    name: "open".to_string(),
                    is_resolved: false,
                    ordinal: 0,
                },
                StateValueInfo {
                    name: "closed".to_string(),
                    is_resolved: true,
                    ordinal: 1,
                },
            ],
        },
        ProjectCustomField {
            id: "assignee".to_string(),
            name: "Assignee".to_string(),
            field_type: "user[1]".to_string(),
            required: false,
            values: vec![],
            state_values: vec![],
        },
        ProjectCustomField {
            id: "labels".to_string(),
            name: "Labels".to_string(),
            field_type: "enum[*]".to_string(),
            required: false,
            values: vec![],
            state_values: vec![],
        },
        ProjectCustomField {
            id: "milestone".to_string(),
            name: "Milestone".to_string(),
            field_type: "enum[1]".to_string(),
            required: false,
            values: vec![],
            state_values: vec![],
        },
    ]
}

/// Parse a GitHub ISO 8601 datetime string
fn parse_github_datetime(dt: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(dt)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Map a GitHub timeline actor onto a [`CommentAuthor`].
///
/// Mirrors the comment author mapping (`login` → `login`, `name` →
/// `Some(login)`) so history and comment authors render identically. A system
/// event with no actor produces `None`.
fn timeline_actor_to_author(actor: Option<GitHubUser>) -> Option<CommentAuthor> {
    actor.map(|u| CommentAuthor {
        login: u.login.clone(),
        name: Some(u.login),
    })
}

/// Convert a GitHub issue timeline (oldest-first) into history events
/// (newest-first).
///
/// GitHub is an event-stream backend: each event records *what happened*, not a
/// before/after diff. For the workflow status field there is no `from` in the
/// payload, so we reconstruct it by walking the events in chronological order
/// while threading a single running `status` string (seeded to `"open"`). Each
/// close/reopen reads the current status as its `from`, then advances it. All
/// other fields follow a status-only-from policy: their `from` stays `None`
/// unless the event itself carries a prior value (only `renamed` does).
///
/// Events whose `created_at` cannot be parsed are skipped rather than
/// fabricated. After the chronological walk the result is sorted newest-first
/// (stable) to match the reporting order used by the other backends.
pub fn github_timeline_to_events(events: Vec<GitHubTimelineEvent>) -> Vec<IssueHistoryEvent> {
    // The seed/initial workflow state. GitHub issues are born `open`.
    let mut status = "open".to_string();
    let mut out = Vec::new();

    for event in events {
        match event {
            GitHubTimelineEvent::Closed { created_at, actor } => {
                let Some(at) = created_at.as_deref().and_then(parse_github_datetime) else {
                    continue;
                };
                out.push(IssueHistoryEvent {
                    at,
                    author: timeline_actor_to_author(actor),
                    field: canonical_field_name("status"),
                    from: Some(status.clone()),
                    to: Some("closed".to_string()),
                });
                status = "closed".to_string();
            }
            GitHubTimelineEvent::Reopened { created_at, actor } => {
                let Some(at) = created_at.as_deref().and_then(parse_github_datetime) else {
                    continue;
                };
                out.push(IssueHistoryEvent {
                    at,
                    author: timeline_actor_to_author(actor),
                    field: canonical_field_name("status"),
                    from: Some(status.clone()),
                    to: Some("open".to_string()),
                });
                status = "open".to_string();
            }
            GitHubTimelineEvent::Assigned {
                created_at,
                actor,
                assignee,
            } => {
                let Some(at) = created_at.as_deref().and_then(parse_github_datetime) else {
                    continue;
                };
                out.push(IssueHistoryEvent {
                    at,
                    author: timeline_actor_to_author(actor),
                    field: "assignee".to_string(),
                    from: None,
                    to: assignee.map(|u| u.login),
                });
            }
            GitHubTimelineEvent::Unassigned {
                created_at,
                actor,
                assignee,
            } => {
                let Some(at) = created_at.as_deref().and_then(parse_github_datetime) else {
                    continue;
                };
                out.push(IssueHistoryEvent {
                    at,
                    author: timeline_actor_to_author(actor),
                    field: "assignee".to_string(),
                    from: assignee.map(|u| u.login),
                    to: None,
                });
            }
            GitHubTimelineEvent::Labeled {
                created_at,
                actor,
                label,
            } => {
                let Some(at) = created_at.as_deref().and_then(parse_github_datetime) else {
                    continue;
                };
                out.push(IssueHistoryEvent {
                    at,
                    author: timeline_actor_to_author(actor),
                    field: "labels".to_string(),
                    from: None,
                    to: label.map(|l| l.name),
                });
            }
            GitHubTimelineEvent::Unlabeled {
                created_at,
                actor,
                label,
            } => {
                let Some(at) = created_at.as_deref().and_then(parse_github_datetime) else {
                    continue;
                };
                out.push(IssueHistoryEvent {
                    at,
                    author: timeline_actor_to_author(actor),
                    field: "labels".to_string(),
                    from: label.map(|l| l.name),
                    to: None,
                });
            }
            GitHubTimelineEvent::Renamed {
                created_at,
                actor,
                rename,
            } => {
                let Some(at) = created_at.as_deref().and_then(parse_github_datetime) else {
                    continue;
                };
                // The `renamed` event is the one event that carries a prior
                // value, so use its from/to directly.
                let (from, to) = match rename {
                    Some(r) => (r.from, r.to),
                    None => (None, None),
                };
                out.push(IssueHistoryEvent {
                    at,
                    author: timeline_actor_to_author(actor),
                    field: "title".to_string(),
                    from,
                    to,
                });
            }
            GitHubTimelineEvent::Milestoned {
                created_at,
                actor,
                milestone,
            } => {
                let Some(at) = created_at.as_deref().and_then(parse_github_datetime) else {
                    continue;
                };
                out.push(IssueHistoryEvent {
                    at,
                    author: timeline_actor_to_author(actor),
                    field: "milestone".to_string(),
                    from: None,
                    to: milestone.map(|m| m.title),
                });
            }
            GitHubTimelineEvent::Demilestoned {
                created_at,
                actor,
                milestone,
            } => {
                let Some(at) = created_at.as_deref().and_then(parse_github_datetime) else {
                    continue;
                };
                out.push(IssueHistoryEvent {
                    at,
                    author: timeline_actor_to_author(actor),
                    field: "milestone".to_string(),
                    from: milestone.map(|m| m.title),
                    to: None,
                });
            }
            GitHubTimelineEvent::Other => {}
        }
    }

    // GitHub returns the timeline oldest-first; expose newest-first to match
    // the reporting order used by the other backends. `sort_by` is stable, so
    // events sharing a timestamp keep their chronological order.
    out.sort_by(|a, b| b.at.cmp(&a.at));
    out
}
