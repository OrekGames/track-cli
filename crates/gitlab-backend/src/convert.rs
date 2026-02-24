//! Model conversions from GitLab types to tracker-core types

use chrono::{DateTime, Utc};
use tracker_core::{
    Comment, CommentAuthor, CustomField, IssueLink, IssueLinkType, IssueTag, LinkedIssue, Project,
    ProjectCustomField, ProjectRef, StateValueInfo, Tag, TagColor, User,
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
    }
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

/// Map a link type name to the GitLab link type string.
///
/// Accepts both GitLab-native names and CLI backend link type names
/// (e.g. "Depend" from the CLI's `--type depends` mapping).
pub fn map_link_type(link_type: &str) -> &str {
    match link_type.to_lowercase().as_str() {
        "relates" | "related" | "relates_to" => "relates_to",
        "blocks" | "depend" | "depends" | "dependency" => "blocks",
        "is_blocked_by" | "blocked" | "blocked_by" | "required" => "is_blocked_by",
        _ => "relates_to",
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
            match hash_tag.to_lowercase().as_str() {
                "unresolved" | "open" | "opened" => state = Some("opened".to_string()),
                "resolved" | "closed" => state = Some("closed".to_string()),
                _ => search_parts.push(*token),
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
}
