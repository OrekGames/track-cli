//! Model conversions from GitHub types to tracker-core types

use chrono::{DateTime, Utc};
use tracker_core::{
    Comment, CommentAuthor, CreateIssue, CustomField, CustomFieldUpdate, Issue, IssueTag, Project,
    ProjectCustomField, ProjectRef, StateValueInfo, Tag, TagColor, UpdateIssue,
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
            CustomFieldUpdate::SingleUser { name, login } if name.to_lowercase() == "assignee" => {
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
    // Extract state from custom fields (CLI sends "State", backends may use "Status")
    let state = update.custom_fields.iter().find_map(|cf| match cf {
        CustomFieldUpdate::State { name, value }
            if name.to_lowercase() == "status" || name.to_lowercase() == "state" =>
        {
            Some(value.clone())
        }
        _ => None,
    });

    let assignees: Vec<String> = update
        .custom_fields
        .iter()
        .filter_map(|cf| match cf {
            CustomFieldUpdate::SingleUser { name, login } if name.to_lowercase() == "assignee" => {
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
            match state.to_lowercase().as_str() {
                "unresolved" | "open" => parts.push("is:open".to_string()),
                "resolved" | "closed" => parts.push("is:closed".to_string()),
                _ => parts.push(format!("label:{}", state)),
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
