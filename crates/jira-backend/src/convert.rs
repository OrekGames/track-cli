//! Model conversions from Jira types to tracker-core types

use chrono::{DateTime, Utc};
use tracker_core::{
    Comment, CommentAuthor, CreateIssue, CustomField, CustomFieldUpdate, Issue, IssueLink,
    IssueLinkType, LinkedIssue, Project, ProjectCustomField, ProjectRef, Tag, UpdateIssue,
};

use crate::models::*;

impl From<JiraIssue> for Issue {
    fn from(j: JiraIssue) -> Self {
        let description = j
            .fields
            .description
            .as_ref()
            .map(adf_to_text)
            .filter(|s| !s.is_empty());

        let is_resolved = j
            .fields
            .status
            .status_category
            .as_ref()
            .map(|c| c.key == "done")
            .unwrap_or(false);

        let mut custom_fields = Vec::new();

        // Map status as a State custom field
        custom_fields.push(CustomField::State {
            name: "Status".to_string(),
            value: Some(j.fields.status.name.clone()),
            is_resolved,
        });

        // Map priority as a SingleEnum custom field
        if let Some(ref priority) = j.fields.priority {
            custom_fields.push(CustomField::SingleEnum {
                name: "Priority".to_string(),
                value: Some(priority.name.clone()),
            });
        }

        // Map assignee as a SingleUser custom field
        custom_fields.push(CustomField::SingleUser {
            name: "Assignee".to_string(),
            login: j
                .fields
                .assignee
                .as_ref()
                .and_then(|u| u.account_id.clone()),
            display_name: j
                .fields
                .assignee
                .as_ref()
                .and_then(|u| u.display_name.clone()),
        });

        // Map issue type
        custom_fields.push(CustomField::SingleEnum {
            name: "Type".to_string(),
            value: Some(j.fields.issuetype.name.clone()),
        });

        Self {
            id: j.id,
            id_readable: j.key,
            summary: j.fields.summary,
            description,
            project: j.fields.project.into(),
            custom_fields,
            tags: j
                .fields
                .labels
                .into_iter()
                .map(|label| Tag {
                    id: label.clone(),
                    name: label,
                })
                .collect(),
            created: parse_jira_datetime(&j.fields.created).unwrap_or_else(Utc::now),
            updated: parse_jira_datetime(&j.fields.updated).unwrap_or_else(Utc::now),
        }
    }
}

impl From<JiraProjectRef> for ProjectRef {
    fn from(p: JiraProjectRef) -> Self {
        Self {
            id: p.id,
            name: p.name,
            short_name: Some(p.key),
        }
    }
}

impl From<JiraProject> for Project {
    fn from(p: JiraProject) -> Self {
        Self {
            id: p.id,
            name: p.name,
            short_name: p.key,
            description: p.description,
        }
    }
}

impl From<JiraComment> for Comment {
    fn from(c: JiraComment) -> Self {
        Self {
            id: c.id,
            text: adf_to_text(&c.body),
            author: c.author.map(|u| CommentAuthor {
                login: u.account_id.unwrap_or_default(),
                name: u.display_name,
            }),
            created: parse_jira_datetime(&c.created),
        }
    }
}

impl From<JiraIssueLink> for IssueLink {
    fn from(l: JiraIssueLink) -> Self {
        let mut issues = Vec::new();

        if let Some(ref outward) = l.outward_issue {
            issues.push(LinkedIssue {
                id: outward.id.clone(),
                id_readable: Some(outward.key.clone()),
                summary: outward.fields.as_ref().and_then(|f| f.summary.clone()),
            });
        }

        if let Some(ref inward) = l.inward_issue {
            issues.push(LinkedIssue {
                id: inward.id.clone(),
                id_readable: Some(inward.key.clone()),
                summary: inward.fields.as_ref().and_then(|f| f.summary.clone()),
            });
        }

        let direction = if l.outward_issue.is_some() && l.inward_issue.is_none() {
            Some("outward".to_string())
        } else if l.inward_issue.is_some() && l.outward_issue.is_none() {
            Some("inward".to_string())
        } else {
            Some("both".to_string())
        };

        Self {
            id: l.id.unwrap_or_default(),
            direction,
            link_type: IssueLinkType {
                id: l.link_type.id.unwrap_or_default(),
                name: l.link_type.name,
                source_to_target: l.link_type.outward,
                target_to_source: l.link_type.inward,
                directed: true,
            },
            issues,
        }
    }
}

/// Convert CreateIssue to Jira format
pub fn create_issue_to_jira(issue: &CreateIssue) -> CreateJiraIssue {
    let description = issue.description.as_ref().map(|d| text_to_adf(d));

    // Extract priority from custom fields if provided
    let priority = issue.custom_fields.iter().find_map(|cf| match cf {
        CustomFieldUpdate::SingleEnum { name, value } if name.to_lowercase() == "priority" => {
            Some(PriorityId {
                id: None,
                name: Some(value.clone()),
            })
        }
        _ => None,
    });

    // Get issue type from custom fields, default to "Task"
    let issue_type = issue
        .custom_fields
        .iter()
        .find_map(|cf| match cf {
            CustomFieldUpdate::SingleEnum { name, value }
                if name.to_lowercase() == "type" || name.to_lowercase() == "issuetype" =>
            {
                Some(value.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| "Task".to_string());

    CreateJiraIssue {
        fields: CreateJiraIssueFields {
            project: ProjectId {
                id: None,
                key: Some(issue.project_id.clone()),
            },
            summary: issue.summary.clone(),
            description,
            issuetype: IssueTypeId {
                id: None,
                name: Some(issue_type),
            },
            priority,
            labels: if issue.tags.is_empty() {
                None
            } else {
                Some(issue.tags.clone())
            },
            parent: None, // Parent is not directly available in CreateIssue
        },
    }
}

/// Convert UpdateIssue to Jira format
pub fn update_issue_to_jira(update: &UpdateIssue) -> UpdateJiraIssue {
    let description = update.description.as_ref().map(|d| text_to_adf(d));

    let priority = update.custom_fields.iter().find_map(|cf| match cf {
        CustomFieldUpdate::SingleEnum { name, value } if name.to_lowercase() == "priority" => {
            Some(PriorityId {
                id: None,
                name: Some(value.clone()),
            })
        }
        _ => None,
    });

    UpdateJiraIssue {
        fields: UpdateJiraIssueFields {
            summary: update.summary.clone(),
            description,
            priority,
            labels: if update.tags.is_empty() {
                None
            } else {
                Some(update.tags.clone())
            },
        },
    }
}

/// Parse Jira datetime string to chrono DateTime
fn parse_jira_datetime(dt: &Option<String>) -> Option<DateTime<Utc>> {
    dt.as_ref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    })
}

/// Map link type from tracker-core to Jira link type name
pub fn map_link_type(link_type: &str) -> &str {
    match link_type.to_lowercase().as_str() {
        "relates" | "related" => "Relates",
        "depends" | "dependency" => "Blocks",
        "required" => "Blocks",
        "duplicates" | "duplicate" => "Duplicate",
        "duplicated-by" => "Duplicate",
        "subtask" | "parent" => "Subtask",
        _ => "Relates",
    }
}

/// Get standard Jira custom fields for a project
pub fn get_standard_custom_fields() -> Vec<ProjectCustomField> {
    vec![
        ProjectCustomField {
            id: "priority".to_string(),
            name: "Priority".to_string(),
            field_type: "enum[1]".to_string(),
            required: false,
        },
        ProjectCustomField {
            id: "assignee".to_string(),
            name: "Assignee".to_string(),
            field_type: "user[1]".to_string(),
            required: false,
        },
        ProjectCustomField {
            id: "status".to_string(),
            name: "Status".to_string(),
            field_type: "state[1]".to_string(),
            required: true,
        },
        ProjectCustomField {
            id: "issuetype".to_string(),
            name: "Type".to_string(),
            field_type: "enum[1]".to_string(),
            required: true,
        },
        ProjectCustomField {
            id: "labels".to_string(),
            name: "Labels".to_string(),
            field_type: "enum[*]".to_string(),
            required: false,
        },
    ]
}
