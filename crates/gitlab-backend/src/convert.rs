//! Model conversions from GitLab types to tracker-core types

use chrono::{DateTime, Utc};
use tracker_core::{
    Comment, CommentAuthor, CustomField, IssueHistoryEvent, IssueLink, IssueLinkType, IssueTag,
    LinkedIssue, Project, ProjectCustomField, ProjectRef, StateValueInfo, Tag, TagColor, User,
    canonical_field_name,
};

use crate::models::*;

/// Convert a GitLab issue to a tracker-core Issue.
///
/// `project_id` is the configured project ID string for the ProjectRef.
pub fn gitlab_issue_to_core(issue: GitLabIssue, project_id: &str) -> tracker_core::Issue {
    let mut custom_fields = Vec::new();

    // Map state as a State custom field
    let is_resolved = issue.state == "closed";
    let state_value = if is_resolved { "Closed" } else { "Open" };
    custom_fields.push(CustomField::State {
        name: "Status".to_string(),
        value: Some(state_value.to_string()),
        is_resolved,
    });

    // Map assignee as a SingleUser custom field
    custom_fields.push(CustomField::SingleUser {
        name: "Assignee".to_string(),
        login: issue.assignee.as_ref().map(|u| u.username.clone()),
        display_name: issue.assignee.as_ref().map(|u| u.name.clone()),
    });

    // Map milestone as a SingleEnum custom field
    if let Some(ref milestone) = issue.milestone {
        custom_fields.push(CustomField::SingleEnum {
            name: "Milestone".to_string(),
            value: Some(milestone.title.clone()),
        });
    }

    if !issue.assignees.is_empty() {
        let value =
            serde_json::to_value(&issue.assignees).expect("GitLab assignees should serialize");
        if let Some(field) = classify_gitlab_extra("assignees", &value) {
            custom_fields.push(field);
        }
    }

    if let Some(author) = issue.author.as_ref() {
        let value = serde_json::to_value(author).expect("GitLab author should serialize");
        if let Some(field) = classify_gitlab_extra("author", &value) {
            custom_fields.push(field);
        }
    }

    // --- Step A: typed promotions for high-value fields. ---
    if let Some(w) = issue.weight {
        custom_fields.push(CustomField::Text {
            name: "Weight".into(),
            value: Some(w.to_string()),
        });
    }
    if let Some(d) = issue.due_date.clone().filter(|s| !s.is_empty()) {
        custom_fields.push(CustomField::Text {
            name: "Due Date".into(),
            value: Some(d),
        });
    }
    custom_fields.push(CustomField::Text {
        name: "Confidential".into(),
        value: Some(issue.confidential.to_string()),
    });

    // --- Step B: surface any remaining API fields losslessly. ---
    // NOISE: fields that are structural/transport noise, not user-facing data.
    const NOISE: &[&str] = &[
        "web_url",
        "_links",
        "references",
        "time_stats",
        "task_completion_status",
        "epic_iid",
        "project_id",
        "id",
        "iid",
        "subscribed",
        "user_notes_count",
        "blocking_issues_count",
        "upvotes",
        "downvotes",
        "merge_requests_count",
        "moved_to_id",
        "service_desk_reply_to",
        "imported",
        "imported_from",
    ];
    // Fields already surfaced as typed/hardcoded variants above.
    let known_titles = [
        "Status",
        "Assignee",
        "Milestone",
        "Weight",
        "Due Date",
        "Confidential",
    ];

    // Sort keys for deterministic output ordering.
    let mut extra_keys: Vec<&String> = issue.extra.keys().collect();
    extra_keys.sort();
    for key in extra_keys {
        if NOISE.contains(&key.as_str()) {
            continue;
        }
        let val = &issue.extra[key];
        if val.is_null() {
            continue;
        }
        let title = canonical_field_name(key);
        if known_titles
            .iter()
            .any(|kt| kt.eq_ignore_ascii_case(&title))
        {
            continue;
        }
        if let Some(field) = classify_gitlab_extra(&title, val) {
            custom_fields.push(field);
        }
    }

    let tags: Vec<Tag> = issue
        .labels
        .iter()
        .map(|label| Tag {
            id: label.clone(),
            name: label.clone(),
        })
        .collect();

    tracker_core::Issue {
        id: issue.id.to_string(),
        id_readable: format!("#{}", issue.iid),
        summary: issue.title,
        description: issue.description.filter(|s| !s.is_empty()),
        project: ProjectRef {
            id: project_id.to_string(),
            name: None,
            short_name: None,
        },
        custom_fields,
        tags,
        created: parse_gitlab_datetime(&issue.created_at).unwrap_or_else(Utc::now),
        updated: parse_gitlab_datetime(&issue.updated_at).unwrap_or_else(Utc::now),
        resolved: parse_gitlab_datetime(&issue.closed_at),
    }
}

/// Classify a single unmodeled GitLab issue field into the most specific
/// [`CustomField`] variant we can prove. Per the projection contract, a
/// present-but-untypeable value falls back to `Unknown { value: Some(raw) }`.
fn classify_gitlab_extra(name: &str, val: &serde_json::Value) -> Option<CustomField> {
    use serde_json::Value;
    match val {
        Value::Null => None,
        Value::String(s) => Some(CustomField::Text {
            name: name.to_string(),
            value: Some(s.clone()),
        }),
        Value::Bool(b) => Some(CustomField::Text {
            name: name.to_string(),
            value: Some(b.to_string()),
        }),
        Value::Number(n) => Some(CustomField::Text {
            name: name.to_string(),
            value: Some(n.to_string()),
        }),
        Value::Array(arr) => classify_array(name, arr),
        Value::Object(obj) => {
            // A user object: surface as SingleUser.
            if obj.contains_key("username") {
                return Some(CustomField::SingleUser {
                    name: name.to_string(),
                    login: obj
                        .get("username")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    display_name: obj.get("name").and_then(|v| v.as_str()).map(String::from),
                });
            }
            // A single-display-key reference (only `name`/`title`): surface that
            // display value as SingleEnum. A richer object carrying sub-fields
            // beyond the lone display key cannot be losslessly flattened, so per
            // the array-of-objects heuristic (applied here to a single object)
            // it is preserved verbatim as Unknown.
            let display = obj
                .get("name")
                .or_else(|| obj.get("title"))
                .and_then(|v| v.as_str());
            let is_single_display_key =
                obj.len() == 1 && (obj.contains_key("name") || obj.contains_key("title"));
            match (display, is_single_display_key) {
                (Some(d), true) => Some(CustomField::SingleEnum {
                    name: name.to_string(),
                    value: Some(d.to_string()),
                }),
                _ => Some(CustomField::Unknown {
                    name: name.to_string(),
                    value: Some(val.clone()),
                }),
            }
        }
    }
}

/// Classify an array field. Per the maintainer-decided array heuristic:
/// - empty -> dropped (`None`)
/// - all plain strings (or single-display-key objects whose only key is
///   `name`/`title`) -> [`CustomField::MultiEnum`]
/// - any rich object (sub-fields beyond a single display key) -> the whole
///   array preserved as [`CustomField::Unknown`].
fn classify_array(name: &str, arr: &[serde_json::Value]) -> Option<CustomField> {
    use serde_json::Value;
    if arr.is_empty() {
        return None;
    }

    let mut values = Vec::with_capacity(arr.len());
    for item in arr {
        match item {
            Value::String(s) => values.push(s.clone()),
            Value::Object(obj) => {
                // Acceptable only if its sole content is a single display key.
                let is_single_display_key =
                    obj.len() == 1 && (obj.contains_key("name") || obj.contains_key("title"));
                if !is_single_display_key {
                    // Rich object: preserve the whole array verbatim.
                    return Some(CustomField::Unknown {
                        name: name.to_string(),
                        value: Some(Value::Array(arr.to_vec())),
                    });
                }
                let display = obj
                    .get("name")
                    .or_else(|| obj.get("title"))
                    .and_then(|v| v.as_str());
                match display {
                    Some(s) => values.push(s.to_string()),
                    // Single display key present but not a string: not plainly
                    // representable -> preserve the whole array.
                    None => {
                        return Some(CustomField::Unknown {
                            name: name.to_string(),
                            value: Some(Value::Array(arr.to_vec())),
                        });
                    }
                }
            }
            // Numbers, bools, nested arrays, nulls: not a plain display list.
            _ => {
                return Some(CustomField::Unknown {
                    name: name.to_string(),
                    value: Some(Value::Array(arr.to_vec())),
                });
            }
        }
    }

    Some(CustomField::MultiEnum {
        name: name.to_string(),
        values,
    })
}

impl From<GitLabNote> for Comment {
    fn from(note: GitLabNote) -> Self {
        Self {
            id: note.id.to_string(),
            text: note.body,
            author: note.author.map(|u| CommentAuthor {
                login: u.username,
                name: Some(u.name),
            }),
            created: parse_gitlab_datetime(&note.created_at),
        }
    }
}

impl From<GitLabProject> for Project {
    fn from(p: GitLabProject) -> Self {
        Self {
            id: p.id.to_string(),
            name: p.name,
            short_name: p
                .path_with_namespace
                .unwrap_or_else(|| p.path.unwrap_or_default()),
            description: p.description,
        }
    }
}

impl From<GitLabLabel> for IssueTag {
    fn from(label: GitLabLabel) -> Self {
        Self {
            id: label.id.to_string(),
            name: label.name,
            color: Some(TagColor {
                id: label.id.to_string(),
                background: Some(label.color),
                foreground: None,
            }),
            issues_count: None,
        }
    }
}

impl From<GitLabUser> for User {
    fn from(u: GitLabUser) -> Self {
        Self {
            id: u.id.to_string(),
            login: Some(u.username),
            display_name: u.name,
        }
    }
}

/// Convert a GitLab issue link to a tracker-core IssueLink.
///
/// The GET endpoint returns linked issues as flat objects with `link_type` metadata.
/// `_current_iid` is kept for API compatibility but is no longer needed since the
/// GET response returns a flat list of linked issues (not source/target pairs).
pub fn gitlab_link_to_core(link: GitLabIssueLink, _current_iid: u64) -> IssueLink {
    let (link_type_info, direction) = match link.link_type.as_str() {
        "blocks" => (
            IssueLinkType {
                id: "blocks".to_string(),
                name: "Blocks".to_string(),
                source_to_target: Some("blocks".to_string()),
                target_to_source: Some("is blocked by".to_string()),
                directed: true,
            },
            "outward",
        ),
        "is_blocked_by" => (
            IssueLinkType {
                id: "is_blocked_by".to_string(),
                name: "Is Blocked By".to_string(),
                source_to_target: Some("is blocked by".to_string()),
                target_to_source: Some("blocks".to_string()),
                directed: true,
            },
            "inward",
        ),
        _ => (
            IssueLinkType {
                id: "relates_to".to_string(),
                name: "Relates".to_string(),
                source_to_target: Some("relates to".to_string()),
                target_to_source: Some("relates to".to_string()),
                directed: false,
            },
            "both",
        ),
    };

    IssueLink {
        id: link.issue_link_id.to_string(),
        direction: Some(direction.to_string()),
        link_type: link_type_info,
        issues: vec![LinkedIssue {
            id: link.id.to_string(),
            id_readable: Some(format!("#{}", link.iid)),
            summary: Some(link.title),
        }],
    }
}

/// Get the 3 standard GitLab link types as tracker-core IssueLinkType values
pub fn get_gitlab_link_types() -> Vec<IssueLinkType> {
    vec![
        IssueLinkType {
            id: "relates_to".to_string(),
            name: "Relates".to_string(),
            source_to_target: Some("relates to".to_string()),
            target_to_source: Some("relates to".to_string()),
            directed: false,
        },
        IssueLinkType {
            id: "blocks".to_string(),
            name: "Blocks".to_string(),
            source_to_target: Some("blocks".to_string()),
            target_to_source: Some("is blocked by".to_string()),
            directed: true,
        },
        IssueLinkType {
            id: "is_blocked_by".to_string(),
            name: "Is Blocked By".to_string(),
            source_to_target: Some("is blocked by".to_string()),
            target_to_source: Some("blocks".to_string()),
            directed: true,
        },
    ]
}

/// Get standard GitLab custom fields for a project
pub fn get_standard_custom_fields() -> Vec<ProjectCustomField> {
    vec![
        ProjectCustomField {
            id: "status".to_string(),
            name: "Status".to_string(),
            field_type: "state[1]".to_string(),
            required: true,
            values: vec!["Open".to_string(), "Closed".to_string()],
            state_values: vec![
                StateValueInfo {
                    name: "Open".to_string(),
                    is_resolved: false,
                    ordinal: 0,
                },
                StateValueInfo {
                    name: "Closed".to_string(),
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

/// Parse GitLab ISO 8601 datetime string to chrono DateTime
fn parse_gitlab_datetime(dt: &Option<String>) -> Option<DateTime<Utc>> {
    dt.as_ref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    })
}

/// Map a GitLab resource-event user onto a [`CommentAuthor`].
///
/// Mirrors the note/comment author mapping (`username` → `login`, `name` →
/// `Some(name)`) so history and comment authors render identically. A
/// system-generated event with no user produces `None`.
fn event_user_to_author(user: Option<GitLabUser>) -> Option<CommentAuthor> {
    user.map(|u| CommentAuthor {
        login: u.username,
        name: Some(u.name),
    })
}

/// Merge GitLab's three resource-event streams into a unified history.
///
/// GitLab is an event-stream backend: state/label/milestone changes live on
/// three separate endpoints, each recording *what happened* rather than a
/// before/after diff.
///
/// For the workflow status field there is no `from` in the payload, so we
/// reconstruct it by walking the state events in chronological order
/// (oldest-first) while threading a single running `status` string (seeded to
/// `"opened"`, the state a GitLab issue is born in). Each state event reads the
/// current status as its `from`, then advances it. Events whose `created_at`
/// cannot be parsed are skipped *without* advancing the running status, so a
/// dropped event never corrupts later `from` values.
///
/// Label and milestone events carry their own value directly: an `add` emits a
/// `to` with no `from`, a `remove` emits a `from` with no `to`. Their `field`
/// stays fixed (`labels` / `milestone`).
///
/// After merging all three streams the result is sorted newest-first (stable)
/// to match the reporting order used by the other backends.
pub fn gitlab_events_to_history_events(
    mut state: Vec<GitLabStateEvent>,
    label: Vec<GitLabLabelEvent>,
    milestone: Vec<GitLabMilestoneEvent>,
) -> Vec<IssueHistoryEvent> {
    let mut out = Vec::new();

    // --- State events: derive `from` by walking chronologically. ---
    // Sort oldest-first so the running-status thread is meaningful; `sort_by`
    // is stable, preserving the API's ordering for events sharing a timestamp.
    state.sort_by(|a, b| {
        let a_at = parse_gitlab_datetime(&a.created_at);
        let b_at = parse_gitlab_datetime(&b.created_at);
        a_at.cmp(&b_at)
    });

    // GitLab issues are born `opened`.
    let mut status = "opened".to_string();
    for event in state {
        let Some(at) = parse_gitlab_datetime(&event.created_at) else {
            // Skip unparseable timestamps without advancing the running status.
            continue;
        };
        let Some(to) = event.state else {
            continue;
        };
        out.push(IssueHistoryEvent {
            at,
            author: event_user_to_author(event.user),
            field: canonical_field_name("state"),
            from: Some(status.clone()),
            to: Some(to.clone()),
        });
        status = to;
    }

    // --- Label events: `add` -> to, `remove` -> from. ---
    for event in label {
        let Some(at) = parse_gitlab_datetime(&event.created_at) else {
            continue;
        };
        let label_name = event.label.map(|l| l.name);
        let (from, to) = match event.action.as_deref() {
            Some("remove") => (label_name, None),
            _ => (None, label_name), // "add" (and any other action) treated as add
        };
        out.push(IssueHistoryEvent {
            at,
            author: event_user_to_author(event.user),
            field: "labels".to_string(),
            from,
            to,
        });
    }

    // --- Milestone events: `add` -> to, `remove` -> from. ---
    for event in milestone {
        let Some(at) = parse_gitlab_datetime(&event.created_at) else {
            continue;
        };
        let title = event.milestone.map(|m| m.title);
        let (from, to) = match event.action.as_deref() {
            Some("remove") => (title, None),
            _ => (None, title),
        };
        out.push(IssueHistoryEvent {
            at,
            author: event_user_to_author(event.user),
            field: "milestone".to_string(),
            from,
            to,
        });
    }

    // Expose newest-first to match the reporting order used by the other
    // backends. `sort_by` is stable, so events sharing a timestamp keep their
    // insertion order.
    out.sort_by(|a, b| b.at.cmp(&a.at));
    out
}

/// Convert a simple tracker-core query to GitLab search params.
///
/// Accepts two formats:
/// - **URL-param**: `state=opened&labels=bug` (used by cache templates)
/// - **Token-based**: `#open label:bug some text` (used by interactive queries)
///
/// URL-param format is detected when the query contains `=` with no whitespace
/// before the first `=`.
///
/// Returns `(search_text, state, labels)`.
pub fn convert_query_to_gitlab_params(query: &str) -> (String, Option<String>, Option<String>) {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return (String::new(), None, None);
    }

    // Detect URL-param format: contains '=' with no whitespace before the first '='
    let first_eq = trimmed.find('=');
    let first_ws = trimmed.find(char::is_whitespace);
    let is_url_param = match (first_eq, first_ws) {
        (Some(eq_pos), Some(ws_pos)) => eq_pos < ws_pos,
        (Some(_), None) => true,
        _ => false,
    };

    if is_url_param {
        parse_url_params(trimmed)
    } else {
        parse_token_query(trimmed)
    }
}

/// Parse URL-param format: `key=value&key=value`
fn parse_url_params(query: &str) -> (String, Option<String>, Option<String>) {
    let mut search_parts = Vec::new();
    let mut state: Option<String> = None;
    let mut labels: Option<String> = None;

    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            match key {
                "state" => state = Some(value.to_string()),
                "labels" => labels = Some(value.to_string()),
                "search" => {
                    if !value.is_empty() {
                        search_parts.push(value.to_string());
                    }
                }
                _ => {} // ignore order_by, sort, assignee_username, etc.
            }
        }
    }

    (search_parts.join(" "), state, labels)
}

/// Parse token-based format: `#open label:bug some text`
fn parse_token_query(query: &str) -> (String, Option<String>, Option<String>) {
    let mut search_parts = Vec::new();
    let mut state: Option<String> = None;
    let mut labels: Option<String> = None;

    let tokens: Vec<&str> = query.split_whitespace().collect();
    for token in &tokens {
        if let Some(hash_tag) = token.strip_prefix('#') {
            if hash_tag.eq_ignore_ascii_case("unresolved")
                || hash_tag.eq_ignore_ascii_case("open")
                || hash_tag.eq_ignore_ascii_case("opened")
            {
                state = Some("opened".to_string());
            } else if hash_tag.eq_ignore_ascii_case("resolved")
                || hash_tag.eq_ignore_ascii_case("closed")
            {
                state = Some("closed".to_string());
            } else {
                search_parts.push(*token);
            }
        } else if let Some(rest) = token.strip_prefix("project:") {
            // Skip project: prefix for GitLab (project is already scoped via project_id)
            let _ = rest;
        } else if let Some(rest) = token.strip_prefix("label:") {
            labels = Some(rest.to_string());
        } else {
            search_parts.push(*token);
        }
    }

    (search_parts.join(" "), state, labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    // URL-param format tests

    #[test]
    fn url_param_state_only() {
        let (search, state, labels) = convert_query_to_gitlab_params("state=opened");
        assert_eq!(search, "");
        assert_eq!(state, Some("opened".to_string()));
        assert_eq!(labels, None);
    }

    #[test]
    fn url_param_state_and_labels() {
        let (search, state, labels) = convert_query_to_gitlab_params("state=opened&labels=bug");
        assert_eq!(search, "");
        assert_eq!(state, Some("opened".to_string()));
        assert_eq!(labels, Some("bug".to_string()));
    }

    #[test]
    fn url_param_with_search() {
        let (search, state, labels) = convert_query_to_gitlab_params("state=opened&search=foo");
        assert_eq!(search, "foo");
        assert_eq!(state, Some("opened".to_string()));
        assert_eq!(labels, None);
    }

    #[test]
    fn url_param_ignores_unknown_keys() {
        let (search, state, labels) =
            convert_query_to_gitlab_params("state=opened&order_by=updated_at");
        assert_eq!(search, "");
        assert_eq!(state, Some("opened".to_string()));
        assert_eq!(labels, None);
    }

    // Token-based format tests

    #[test]
    fn token_hash_open() {
        let (search, state, labels) = convert_query_to_gitlab_params("#open");
        assert_eq!(search, "");
        assert_eq!(state, Some("opened".to_string()));
        assert_eq!(labels, None);
    }

    #[test]
    fn token_hash_unresolved() {
        let (search, state, labels) = convert_query_to_gitlab_params("#unresolved");
        assert_eq!(search, "");
        assert_eq!(state, Some("opened".to_string()));
        assert_eq!(labels, None);
    }

    #[test]
    fn token_label() {
        let (search, state, labels) = convert_query_to_gitlab_params("label:bug");
        assert_eq!(search, "");
        assert_eq!(state, None);
        assert_eq!(labels, Some("bug".to_string()));
    }

    #[test]
    fn token_free_text() {
        let (search, state, labels) = convert_query_to_gitlab_params("hello world");
        assert_eq!(search, "hello world");
        assert_eq!(state, None);
        assert_eq!(labels, None);
    }

    #[test]
    fn empty_string() {
        let (search, state, labels) = convert_query_to_gitlab_params("");
        assert_eq!(search, "");
        assert_eq!(state, None);
        assert_eq!(labels, None);
    }

    // gitlab_link_to_core tests

    use crate::models::issue::GitLabIssueLink;

    #[test]
    fn test_gitlab_link_to_core_blocks() {
        let link = GitLabIssueLink {
            id: 123,
            iid: 456,
            title: "Blocking Issue".to_string(),
            issue_link_id: 789,
            link_type: "blocks".to_string(),
        };

        let core_link = gitlab_link_to_core(link, 100);

        assert_eq!(core_link.id, "789");
        assert_eq!(core_link.direction, Some("outward".to_string()));
        assert_eq!(core_link.link_type.id, "blocks");
        assert_eq!(core_link.link_type.name, "Blocks");
        assert_eq!(core_link.link_type.directed, true);
        assert_eq!(core_link.issues.len(), 1);
        assert_eq!(core_link.issues[0].id, "123");
        assert_eq!(core_link.issues[0].id_readable, Some("#456".to_string()));
        assert_eq!(
            core_link.issues[0].summary,
            Some("Blocking Issue".to_string())
        );
    }

    #[test]
    fn test_gitlab_link_to_core_is_blocked_by() {
        let link = GitLabIssueLink {
            id: 124,
            iid: 457,
            title: "Blocked Issue".to_string(),
            issue_link_id: 790,
            link_type: "is_blocked_by".to_string(),
        };

        let core_link = gitlab_link_to_core(link, 100);

        assert_eq!(core_link.id, "790");
        assert_eq!(core_link.direction, Some("inward".to_string()));
        assert_eq!(core_link.link_type.id, "is_blocked_by");
        assert_eq!(core_link.link_type.name, "Is Blocked By");
        assert_eq!(core_link.link_type.directed, true);
        assert_eq!(core_link.issues.len(), 1);
        assert_eq!(core_link.issues[0].id, "124");
        assert_eq!(core_link.issues[0].id_readable, Some("#457".to_string()));
        assert_eq!(
            core_link.issues[0].summary,
            Some("Blocked Issue".to_string())
        );
    }

    #[test]
    fn test_gitlab_link_to_core_relates_to() {
        let link = GitLabIssueLink {
            id: 125,
            iid: 458,
            title: "Related Issue".to_string(),
            issue_link_id: 791,
            link_type: "relates_to".to_string(),
        };

        let core_link = gitlab_link_to_core(link, 100);

        assert_eq!(core_link.id, "791");
        assert_eq!(core_link.direction, Some("both".to_string()));
        assert_eq!(core_link.link_type.id, "relates_to");
        assert_eq!(core_link.link_type.name, "Relates");
        assert_eq!(core_link.link_type.directed, false);
        assert_eq!(core_link.issues.len(), 1);
        assert_eq!(core_link.issues[0].id, "125");
        assert_eq!(core_link.issues[0].id_readable, Some("#458".to_string()));
        assert_eq!(
            core_link.issues[0].summary,
            Some("Related Issue".to_string())
        );
    }

    #[test]
    fn test_gitlab_link_to_core_unknown() {
        let link = GitLabIssueLink {
            id: 126,
            iid: 459,
            title: "Unknown Link Type Issue".to_string(),
            issue_link_id: 792,
            link_type: "unknown_type".to_string(),
        };

        let core_link = gitlab_link_to_core(link, 100);

        // Unknown link types default to "relates_to"
        assert_eq!(core_link.id, "792");
        assert_eq!(core_link.direction, Some("both".to_string()));
        assert_eq!(core_link.link_type.id, "relates_to");
        assert_eq!(core_link.link_type.name, "Relates");
        assert_eq!(core_link.link_type.directed, false);
        assert_eq!(core_link.issues.len(), 1);
        assert_eq!(core_link.issues[0].id, "126");
        assert_eq!(core_link.issues[0].id_readable, Some("#459".to_string()));
        assert_eq!(
            core_link.issues[0].summary,
            Some("Unknown Link Type Issue".to_string())
        );
    }

    // ==================== gitlab_events_to_history_events tests ====================

    use crate::models::{
        GitLabEventLabel, GitLabEventMilestone, GitLabLabelEvent, GitLabMilestoneEvent,
        GitLabStateEvent, GitLabUser,
    };

    fn user(username: &str) -> GitLabUser {
        GitLabUser {
            id: 1,
            username: username.to_string(),
            name: format!("{} Name", username),
            extra: Default::default(),
        }
    }

    fn state_event(id: u64, at: &str, state: &str, username: Option<&str>) -> GitLabStateEvent {
        GitLabStateEvent {
            id,
            user: username.map(user),
            created_at: Some(at.to_string()),
            state: Some(state.to_string()),
        }
    }

    #[test]
    fn state_events_derive_from_chronologically() {
        // opened -> closed -> reopened, given out of order to exercise the sort.
        let state = vec![
            state_event(3, "2024-03-03T00:00:00Z", "reopened", Some("carol")),
            state_event(1, "2024-01-01T00:00:00Z", "opened", Some("alice")),
            state_event(2, "2024-02-02T00:00:00Z", "closed", Some("bob")),
        ];

        let events = gitlab_events_to_history_events(state, vec![], vec![]);

        assert_eq!(events.len(), 3);
        // Newest-first ordering.
        assert_eq!(events[0].field, "status");
        assert_eq!(events[0].from.as_deref(), Some("closed"));
        assert_eq!(events[0].to.as_deref(), Some("reopened"));
        assert_eq!(events[0].author.as_ref().unwrap().login, "carol");

        assert_eq!(events[1].field, "status");
        assert_eq!(events[1].from.as_deref(), Some("opened"));
        assert_eq!(events[1].to.as_deref(), Some("closed"));
        assert_eq!(events[1].author.as_ref().unwrap().login, "bob");

        // The first transition's `from` is seeded to "opened".
        assert_eq!(events[2].field, "status");
        assert_eq!(events[2].from.as_deref(), Some("opened"));
        assert_eq!(events[2].to.as_deref(), Some("opened"));
        assert_eq!(events[2].author.as_ref().unwrap().login, "alice");
    }

    #[test]
    fn unparseable_state_timestamp_is_skipped_without_advancing_status() {
        // The middle event has a bad timestamp; it must be dropped and must NOT
        // advance the running status, so the final transition's `from` is still
        // "opened" (the seed) rather than "closed".
        let state = vec![
            GitLabStateEvent {
                id: 1,
                user: Some(user("alice")),
                created_at: Some("not-a-date".to_string()),
                state: Some("closed".to_string()),
            },
            state_event(2, "2024-02-02T00:00:00Z", "reopened", Some("bob")),
        ];

        let events = gitlab_events_to_history_events(state, vec![], vec![]);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].field, "status");
        assert_eq!(events[0].from.as_deref(), Some("opened"));
        assert_eq!(events[0].to.as_deref(), Some("reopened"));
    }

    #[test]
    fn label_add_and_remove_produce_correct_from_to() {
        let label = vec![
            GitLabLabelEvent {
                id: 1,
                user: Some(user("alice")),
                created_at: Some("2024-01-01T00:00:00Z".to_string()),
                action: Some("add".to_string()),
                label: Some(GitLabEventLabel {
                    name: "bug".to_string(),
                }),
            },
            GitLabLabelEvent {
                id: 2,
                user: Some(user("bob")),
                created_at: Some("2024-01-02T00:00:00Z".to_string()),
                action: Some("remove".to_string()),
                label: Some(GitLabEventLabel {
                    name: "bug".to_string(),
                }),
            },
        ];

        let events = gitlab_events_to_history_events(vec![], label, vec![]);

        assert_eq!(events.len(), 2);
        // Newest-first: the remove sorts ahead of the add.
        assert_eq!(events[0].field, "labels");
        assert_eq!(events[0].from.as_deref(), Some("bug"));
        assert_eq!(events[0].to, None);

        assert_eq!(events[1].field, "labels");
        assert_eq!(events[1].from, None);
        assert_eq!(events[1].to.as_deref(), Some("bug"));
    }

    #[test]
    fn milestone_add_and_remove_produce_correct_from_to() {
        let milestone = vec![
            GitLabMilestoneEvent {
                id: 1,
                user: Some(user("alice")),
                created_at: Some("2024-01-01T00:00:00Z".to_string()),
                action: Some("add".to_string()),
                milestone: Some(GitLabEventMilestone {
                    title: "v1.0".to_string(),
                }),
            },
            GitLabMilestoneEvent {
                id: 2,
                user: Some(user("bob")),
                created_at: Some("2024-01-02T00:00:00Z".to_string()),
                action: Some("remove".to_string()),
                milestone: Some(GitLabEventMilestone {
                    title: "v1.0".to_string(),
                }),
            },
        ];

        let events = gitlab_events_to_history_events(vec![], vec![], milestone);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].field, "milestone");
        assert_eq!(events[0].from.as_deref(), Some("v1.0"));
        assert_eq!(events[0].to, None);

        assert_eq!(events[1].field, "milestone");
        assert_eq!(events[1].from, None);
        assert_eq!(events[1].to.as_deref(), Some("v1.0"));
    }

    #[test]
    fn mixed_events_merge_and_sort_newest_first() {
        let state = vec![
            state_event(1, "2024-01-01T00:00:00Z", "opened", Some("alice")),
            state_event(2, "2024-01-05T00:00:00Z", "closed", Some("alice")),
        ];
        let label = vec![GitLabLabelEvent {
            id: 1,
            user: Some(user("bob")),
            created_at: Some("2024-01-03T00:00:00Z".to_string()),
            action: Some("add".to_string()),
            label: Some(GitLabEventLabel {
                name: "bug".to_string(),
            }),
        }];
        let milestone = vec![GitLabMilestoneEvent {
            id: 1,
            user: Some(user("carol")),
            created_at: Some("2024-01-04T00:00:00Z".to_string()),
            action: Some("add".to_string()),
            milestone: Some(GitLabEventMilestone {
                title: "v1.0".to_string(),
            }),
        }];

        let events = gitlab_events_to_history_events(state, label, milestone);

        // All four events present, sorted newest-first across all three streams.
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].field, "status"); // 01-05 closed
        assert_eq!(events[0].to.as_deref(), Some("closed"));
        assert_eq!(events[1].field, "milestone"); // 01-04
        assert_eq!(events[2].field, "labels"); // 01-03
        assert_eq!(events[3].field, "status"); // 01-01 opened
        assert_eq!(events[3].to.as_deref(), Some("opened"));
    }

    #[test]
    fn null_user_yields_no_author() {
        let state = vec![state_event(1, "2024-01-01T00:00:00Z", "closed", None)];

        let events = gitlab_events_to_history_events(state, vec![], vec![]);

        assert_eq!(events.len(), 1);
        assert!(events[0].author.is_none());
    }

    #[test]
    fn null_label_is_tolerated() {
        let label = vec![GitLabLabelEvent {
            id: 1,
            user: Some(user("alice")),
            created_at: Some("2024-01-01T00:00:00Z".to_string()),
            action: Some("add".to_string()),
            label: None,
        }];

        let events = gitlab_events_to_history_events(vec![], label, vec![]);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].field, "labels");
        assert_eq!(events[0].from, None);
        assert_eq!(events[0].to, None);
    }

    // ==================== custom-field projection tests ====================

    use crate::models::issue::GitLabIssue;
    use serde_json::json;

    /// Deserialize a GitLabIssue from JSON, filling in the minimal required
    /// fields and merging the supplied extra keys.
    fn issue_from_extra(extra: serde_json::Value) -> GitLabIssue {
        let mut base = json!({
            "id": 1,
            "iid": 10,
            "project_id": 100,
            "title": "Test",
            "state": "opened",
        });
        if let serde_json::Value::Object(extra_map) = extra {
            let base_map = base.as_object_mut().unwrap();
            for (k, v) in extra_map {
                base_map.insert(k, v);
            }
        }
        serde_json::from_value(base).expect("GitLabIssue should deserialize")
    }

    fn find_text<'a>(fields: &'a [CustomField], name: &str) -> Option<&'a Option<String>> {
        fields.iter().find_map(|f| match f {
            CustomField::Text { name: n, value } if n == name => Some(value),
            _ => None,
        })
    }

    fn fields_named<'a>(fields: &'a [CustomField], name: &str) -> Vec<&'a CustomField> {
        fields.iter().filter(|f| field_name(f) == name).collect()
    }

    #[test]
    fn lossless_projection_promotes_and_surfaces() {
        let issue = issue_from_extra(json!({
            "weight": 5,
            "due_date": "2024-06-01",
            "confidential": true,
            "discussion_locked": true,
            "epic": { "id": 7, "iid": 2, "title": "Q3" },
            "iteration": { "title": "Sprint 4" },
        }));

        let core = gitlab_issue_to_core(issue, "100");
        let fields = &core.custom_fields;

        // Typed promotions.
        assert_eq!(find_text(fields, "Weight"), Some(&Some("5".to_string())));
        assert_eq!(
            find_text(fields, "Due Date"),
            Some(&Some("2024-06-01".to_string()))
        );
        assert_eq!(
            find_text(fields, "Confidential"),
            Some(&Some("true".to_string()))
        );

        // Unmodeled scalar surfaced as Text.
        assert_eq!(
            find_text(fields, "discussion_locked"),
            Some(&Some("true".to_string()))
        );

        // Unmodeled rich object (id+iid+title beyond a single display key) is
        // preserved verbatim as Unknown.
        let epic = fields
            .iter()
            .find_map(|f| match f {
                CustomField::Unknown { name, value } if name == "epic" => Some(value),
                _ => None,
            })
            .expect("epic should be Unknown");
        assert_eq!(
            epic.as_ref().unwrap(),
            &json!({ "id": 7, "iid": 2, "title": "Q3" })
        );

        // A single-display-key object collapses to its display value (SingleEnum).
        let iteration = fields.iter().find_map(|f| match f {
            CustomField::SingleEnum { name, value } if name == "iteration" => Some(value),
            _ => None,
        });
        assert_eq!(iteration, Some(&Some("Sprint 4".to_string())));
    }

    #[test]
    fn no_duplication_of_typed_fields() {
        let issue = issue_from_extra(json!({
            "weight": 5,
            "milestone": { "id": 1, "iid": 1, "title": "v1.0" },
        }));

        let core = gitlab_issue_to_core(issue, "100");
        let fields = &core.custom_fields;

        let weight_count = fields
            .iter()
            .filter(|f| matches!(f, CustomField::Text { name, .. } if name == "Weight"))
            .count();
        assert_eq!(weight_count, 1, "Weight should appear exactly once");

        let milestone_count = fields
            .iter()
            .filter(|f| matches!(f, CustomField::SingleEnum { name, .. } if name == "Milestone"))
            .count();
        assert_eq!(milestone_count, 1, "Milestone should appear exactly once");
    }

    #[test]
    fn named_author_consumed_before_extra_is_surfaced() {
        let issue = issue_from_extra(json!({
            "author": {
                "id": 9,
                "username": "reporter",
                "name": "Reporter Name",
                "web_url": "https://gitlab.example/reporter"
            }
        }));
        assert!(
            !issue.extra.contains_key("author"),
            "named author should be consumed before flatten extra"
        );

        let core = gitlab_issue_to_core(issue, "100");

        let matches = fields_named(&core.custom_fields, "author");
        assert_eq!(matches.len(), 1);
        match matches[0] {
            CustomField::SingleUser {
                login,
                display_name,
                ..
            } => {
                assert_eq!(login.as_deref(), Some("reporter"));
                assert_eq!(display_name.as_deref(), Some("Reporter Name"));
            }
            other => panic!("expected SingleUser, got {:?}", other),
        }
    }

    #[test]
    fn named_assignees_are_preserved_without_replacing_assignee() {
        let assignees = json!([
            {
                "id": 7,
                "username": "alice",
                "name": "Alice Name",
                "web_url": "https://gitlab.example/alice"
            },
            {
                "id": 8,
                "username": "bob",
                "name": "Bob Name",
                "web_url": "https://gitlab.example/bob"
            }
        ]);
        let issue = issue_from_extra(json!({
            "assignee": {
                "id": 7,
                "username": "alice",
                "name": "Alice Name"
            },
            "assignees": assignees.clone()
        }));
        assert!(
            !issue.extra.contains_key("assignees"),
            "named assignees should be consumed before flatten extra"
        );

        let core = gitlab_issue_to_core(issue, "100");

        let assignee = fields_named(&core.custom_fields, "Assignee");
        assert_eq!(assignee.len(), 1);
        match assignee[0] {
            CustomField::SingleUser { login, .. } => assert_eq!(login.as_deref(), Some("alice")),
            other => panic!("expected SingleUser, got {:?}", other),
        }

        let matches = fields_named(&core.custom_fields, "assignees");
        assert_eq!(matches.len(), 1);
        match matches[0] {
            CustomField::Unknown { value, .. } => {
                assert_eq!(value.as_ref(), Some(&assignees));
            }
            other => panic!("expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn noise_fields_are_not_surfaced() {
        let issue = issue_from_extra(json!({
            "web_url": "https://example.com/issues/10",
            "_links": { "self": "https://example.com" },
            "references": { "short": "#10" },
            "user_notes_count": 3,
            "upvotes": 2,
        }));

        let core = gitlab_issue_to_core(issue, "100");
        for noise in [
            "web_url",
            "_links",
            "references",
            "user_notes_count",
            "upvotes",
        ] {
            assert!(
                !core.custom_fields.iter().any(|f| field_name(f) == noise),
                "noise field {noise} should not be surfaced"
            );
        }
    }

    #[test]
    fn null_values_are_skipped() {
        let issue = issue_from_extra(json!({
            "some_field": serde_json::Value::Null,
        }));

        let core = gitlab_issue_to_core(issue, "100");
        assert!(
            !core
                .custom_fields
                .iter()
                .any(|f| field_name(f) == "some_field"),
            "null-valued field should be skipped"
        );
    }

    #[test]
    fn array_heuristic_strings_vs_rich_objects() {
        // Plain string array -> MultiEnum.
        let issue = issue_from_extra(json!({ "tag_list": ["a", "b"] }));
        let core = gitlab_issue_to_core(issue, "100");
        let values = core
            .custom_fields
            .iter()
            .find_map(|f| match f {
                CustomField::MultiEnum { name, values } if name == "tag_list" => Some(values),
                _ => None,
            })
            .expect("string array should be MultiEnum");
        assert_eq!(values, &vec!["a".to_string(), "b".to_string()]);

        // Array of rich objects -> Unknown preserving the whole array.
        let issue = issue_from_extra(json!({ "things": [{ "id": 1, "state": "x" }] }));
        let core = gitlab_issue_to_core(issue, "100");
        let preserved = core
            .custom_fields
            .iter()
            .find_map(|f| match f {
                CustomField::Unknown { name, value } if name == "things" => Some(value),
                _ => None,
            })
            .expect("rich-object array should be Unknown");
        assert_eq!(
            preserved.as_ref().unwrap(),
            &json!([{ "id": 1, "state": "x" }])
        );
    }

    /// Helper: the `name` of any CustomField variant.
    fn field_name(f: &CustomField) -> &str {
        match f {
            CustomField::SingleEnum { name, .. }
            | CustomField::State { name, .. }
            | CustomField::SingleUser { name, .. }
            | CustomField::Text { name, .. }
            | CustomField::MultiEnum { name, .. }
            | CustomField::Unknown { name, .. } => name,
        }
    }
}
