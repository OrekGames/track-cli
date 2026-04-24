//! Model conversions from Jira types to tracker-core types

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use tracker_core::{
    Comment, CommentAuthor, CreateIssue, CustomField, CustomFieldUpdate, Issue, IssueLink,
    IssueLinkType, LinkedIssue, Project, ProjectCustomField, ProjectRef, StateValueInfo, Tag,
    UpdateIssue, User,
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

/// Convert JiraIssueLinkType to tracker-core IssueLinkType
impl From<JiraIssueLinkType> for IssueLinkType {
    fn from(lt: JiraIssueLinkType) -> Self {
        Self {
            id: lt.id.unwrap_or_default(),
            name: lt.name,
            source_to_target: lt.outward,
            target_to_source: lt.inward,
            directed: true, // Jira links are typically directional
        }
    }
}

/// Convert JiraUser to tracker-core User
impl From<JiraUser> for User {
    fn from(u: JiraUser) -> Self {
        Self {
            id: u.account_id.clone().unwrap_or_default(),
            login: u.account_id,
            display_name: u.display_name.unwrap_or_else(|| "Unknown".to_string()),
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

/// Convert CreateIssue to Jira format.
/// When `jira_fields` is provided, custom field updates are resolved to Jira field IDs
/// and included in the request. Without it, only standard fields (priority, type, labels) are sent.
pub fn create_issue_to_jira(issue: &CreateIssue, jira_fields: &[JiraField]) -> CreateJiraIssue {
    let description = issue.description.as_ref().map(|d| markdown_to_adf(d));

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

    let extra = resolve_extra_fields(&issue.custom_fields, jira_fields);

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
            parent: issue.parent.as_ref().map(|key| ParentId {
                id: None,
                key: Some(key.clone()),
            }),
            extra,
        },
    }
}

/// Convert UpdateIssue to Jira format.
/// When `jira_fields` is provided, custom field updates are resolved to Jira field IDs
/// and included in the request. Without it, only standard fields (priority, labels) are sent.
pub fn update_issue_to_jira(update: &UpdateIssue, jira_fields: &[JiraField]) -> UpdateJiraIssue {
    let description = update.description.as_ref().map(|d| markdown_to_adf(d));

    let priority = update.custom_fields.iter().find_map(|cf| match cf {
        CustomFieldUpdate::SingleEnum { name, value } if name.to_lowercase() == "priority" => {
            Some(PriorityId {
                id: None,
                name: Some(value.clone()),
            })
        }
        _ => None,
    });

    let extra = resolve_extra_fields(&update.custom_fields, jira_fields);

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
            parent: update.parent.as_ref().map(|key| ParentId {
                id: None,
                key: Some(key.clone()),
            }),
            extra,
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

/// Get standard Jira custom fields for a project
pub fn get_standard_custom_fields() -> Vec<ProjectCustomField> {
    use tracker_core::StateValueInfo;

    vec![
        ProjectCustomField {
            id: "priority".to_string(),
            name: "Priority".to_string(),
            field_type: "enum[1]".to_string(),
            required: false,
            values: vec![
                "Highest".to_string(),
                "High".to_string(),
                "Medium".to_string(),
                "Low".to_string(),
                "Lowest".to_string(),
            ],
            state_values: vec![],
        },
        ProjectCustomField {
            id: "assignee".to_string(),
            name: "Assignee".to_string(),
            field_type: "user[1]".to_string(),
            required: false,
            values: vec![], // Users are fetched separately
            state_values: vec![],
        },
        ProjectCustomField {
            id: "status".to_string(),
            name: "Status".to_string(),
            field_type: "state[1]".to_string(),
            required: true,
            values: vec![
                "To Do".to_string(),
                "In Progress".to_string(),
                "Done".to_string(),
            ],
            state_values: vec![
                StateValueInfo {
                    name: "To Do".to_string(),
                    is_resolved: false,
                    ordinal: 0,
                },
                StateValueInfo {
                    name: "In Progress".to_string(),
                    is_resolved: false,
                    ordinal: 1,
                },
                StateValueInfo {
                    name: "Done".to_string(),
                    is_resolved: true,
                    ordinal: 2,
                },
            ],
        },
        ProjectCustomField {
            id: "issuetype".to_string(),
            name: "Type".to_string(),
            field_type: "enum[1]".to_string(),
            required: true,
            values: vec![
                "Task".to_string(),
                "Bug".to_string(),
                "Story".to_string(),
                "Epic".to_string(),
                "Subtask".to_string(),
            ],
            state_values: vec![],
        },
        ProjectCustomField {
            id: "labels".to_string(),
            name: "Labels".to_string(),
            field_type: "enum[*]".to_string(),
            required: false,
            values: vec![], // Labels are created dynamically
            state_values: vec![],
        },
    ]
}

/// Convert a JiraField to a tracker-core ProjectCustomField.
/// Maps Jira schema types to the internal type conventions used by the CLI.
pub fn jira_field_to_project_custom_field(field: &JiraField) -> ProjectCustomField {
    let field_type = match &field.schema {
        Some(schema) => match schema.field_type.as_str() {
            "number" => "number".to_string(),
            "string" => "string".to_string(),
            "user" => "user[1]".to_string(),
            "array" => match schema.items.as_deref() {
                Some("string") => "enum[*]".to_string(),
                Some("option") => "enum[*]".to_string(),
                Some("user") => "user[*]".to_string(),
                _ => "enum[*]".to_string(),
            },
            "option" => "enum[1]".to_string(),
            _ => schema.field_type.clone(),
        },
        None => "string".to_string(),
    };

    ProjectCustomField {
        id: field.id.clone(),
        name: field.name.clone(),
        field_type,
        required: false,
        values: vec![],
        state_values: vec![],
    }
}

/// Merge standard (hardcoded) fields with instance-level fields from the API.
/// Standard fields take precedence since they include enum values.
pub fn merge_fields(
    standard: Vec<ProjectCustomField>,
    instance: Vec<ProjectCustomField>,
) -> Vec<ProjectCustomField> {
    let mut result = standard;
    let existing_names: Vec<String> = result.iter().map(|f| f.name.to_lowercase()).collect();

    for field in instance {
        if !existing_names.contains(&field.name.to_lowercase()) {
            result.push(field);
        }
    }
    result
}

/// Flatten per-issue-type statuses into a single ordered, deduplicated list
/// for the project's Status custom field.
pub fn flatten_project_statuses(
    per_issue_type: &[ProjectIssueTypeStatuses],
) -> (Vec<String>, Vec<StateValueInfo>) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut ordered: Vec<&ProjectStatus> = Vec::new();

    for group in per_issue_type {
        for st in &group.statuses {
            if seen.insert(st.id.clone()) {
                ordered.push(st);
            }
        }
    }

    let values = ordered.iter().map(|s| s.name.clone()).collect();
    let state_values = ordered
        .iter()
        .enumerate()
        .map(|(i, s)| StateValueInfo {
            name: s.name.clone(),
            is_resolved: s
                .status_category
                .as_ref()
                .map(|c| c.key == "done")
                .unwrap_or(false),
            ordinal: i as i32,
        })
        .collect();

    (values, state_values)
}

/// Build a name → field ID lookup from JiraField metadata.
pub fn build_field_id_map(fields: &[JiraField]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for field in fields {
        map.insert(field.name.to_lowercase(), field.id.clone());
    }
    // Also insert known standard field name mappings
    map.insert("priority".to_string(), "priority".to_string());
    map.insert("assignee".to_string(), "assignee".to_string());
    map.insert("status".to_string(), "status".to_string());
    map.insert("type".to_string(), "issuetype".to_string());
    map.insert("labels".to_string(), "labels".to_string());
    map
}

/// Convert a custom field value to the appropriate JSON representation
/// based on the field's Jira schema type.
fn custom_field_to_json(
    field_id: &str,
    value: &str,
    schema: Option<&JiraFieldSchema>,
) -> serde_json::Value {
    match schema.map(|s| s.field_type.as_str()) {
        Some("number") => value
            .parse::<f64>()
            .map(|n| serde_json::Value::Number(serde_json::Number::from_f64(n).unwrap()))
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
        Some("option") => serde_json::json!({ "value": value }),
        Some("array") => {
            let items_type = schema.and_then(|s| s.items.as_deref());
            let values: Vec<&str> = value.split(',').map(|v| v.trim()).collect();
            match items_type {
                Some("option") => {
                    let items: Vec<serde_json::Value> = values
                        .iter()
                        .map(|v| serde_json::json!({ "value": *v }))
                        .collect();
                    serde_json::Value::Array(items)
                }
                _ => {
                    let items: Vec<serde_json::Value> = values
                        .iter()
                        .map(|v| serde_json::Value::String(v.to_string()))
                        .collect();
                    serde_json::Value::Array(items)
                }
            }
        }
        _ => {
            // Default: if it starts with "customfield_", try number first
            if field_id.starts_with("customfield_")
                && let Ok(n) = value.parse::<f64>()
            {
                return serde_json::Value::Number(serde_json::Number::from_f64(n).unwrap());
            }
            serde_json::Value::String(value.to_string())
        }
    }
}

/// Field names that are handled by the strongly-typed JiraFields struct
/// (priority, assignee, labels, issuetype) OR by the /transitions endpoint
/// (status/state). They must never be forwarded as "extra" custom fields,
/// even if a Jira project defines a custom field with the same display name.
const RESERVED_FIELD_NAMES: &[&str] = &[
    "priority",
    "assignee",
    "status",
    "state", // alias used by CustomFieldUpdate::State
    "type",
    "issuetype",
    "labels",
];

fn is_reserved_field(name: &str) -> bool {
    RESERVED_FIELD_NAMES.contains(&name.to_lowercase().as_str())
}

/// Resolve custom field updates to Jira extra fields using field metadata.
/// Returns a map of field_id → JSON value for fields not handled by the standard struct.
pub fn resolve_extra_fields(
    custom_fields: &[CustomFieldUpdate],
    jira_fields: &[JiraField],
) -> HashMap<String, serde_json::Value> {
    let field_id_map = build_field_id_map(jira_fields);
    let schema_map: HashMap<&str, &JiraFieldSchema> = jira_fields
        .iter()
        .filter_map(|f| f.schema.as_ref().map(|s| (f.id.as_str(), s)))
        .collect();

    let mut extra = HashMap::new();

    for cf in custom_fields {
        let (name, value) = match cf {
            CustomFieldUpdate::SingleEnum { name, value } => (name.as_str(), value.as_str()),
            CustomFieldUpdate::State { name, value } => (name.as_str(), value.as_str()),
            CustomFieldUpdate::SingleUser { name, login } => (name.as_str(), login.as_str()),
            CustomFieldUpdate::MultiEnum { name, values } => {
                // Handle multi-enum specially
                let joined = values.join(",");
                if is_reserved_field(name) {
                    continue;
                }
                if let Some(field_id) = field_id_map.get(&name.to_lowercase()) {
                    let schema = schema_map.get(field_id.as_str()).copied();
                    extra.insert(
                        field_id.clone(),
                        custom_field_to_json(field_id, &joined, schema),
                    );
                }
                continue;
            }
        };

        // Skip standard fields handled by the struct
        if is_reserved_field(name) {
            continue;
        }

        if let Some(field_id) = field_id_map.get(&name.to_lowercase()) {
            let schema = schema_map.get(field_id.as_str()).copied();
            extra.insert(
                field_id.clone(),
                custom_field_to_json(field_id, value, schema),
            );
        }
    }

    extra
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracker_core::{CreateIssue, UpdateIssue};

    #[test]
    fn create_issue_to_jira_maps_parent() {
        let issue = CreateIssue {
            project_id: "PROJ".to_string(),
            summary: "Child task".to_string(),
            description: None,
            custom_fields: vec![],
            tags: vec![],
            parent: Some("PROJ-100".to_string()),
        };

        let jira = create_issue_to_jira(&issue, &[]);
        let parent = jira.fields.parent.expect("parent should be set");
        assert_eq!(parent.key.as_deref(), Some("PROJ-100"));
        assert!(parent.id.is_none());
    }

    #[test]
    fn create_issue_to_jira_omits_parent_when_none() {
        let issue = CreateIssue {
            project_id: "PROJ".to_string(),
            summary: "Regular task".to_string(),
            description: None,
            custom_fields: vec![],
            tags: vec![],
            parent: None,
        };

        let jira = create_issue_to_jira(&issue, &[]);
        assert!(jira.fields.parent.is_none());
    }

    #[test]
    fn update_issue_to_jira_maps_parent() {
        let update = UpdateIssue {
            summary: None,
            description: None,
            custom_fields: vec![],
            tags: vec![],
            parent: Some("PROJ-200".to_string()),
        };

        let jira = update_issue_to_jira(&update, &[]);
        let parent = jira.fields.parent.expect("parent should be set");
        assert_eq!(parent.key.as_deref(), Some("PROJ-200"));
    }

    #[test]
    fn update_issue_to_jira_parent_serializes_correctly() {
        let update = UpdateIssue {
            summary: None,
            description: None,
            custom_fields: vec![],
            tags: vec![],
            parent: Some("DS-100".to_string()),
        };

        let jira = update_issue_to_jira(&update, &[]);
        let json = serde_json::to_value(&jira).unwrap();

        // parent.key should be present, parent.id should be omitted
        let parent = &json["fields"]["parent"];
        assert_eq!(parent["key"], "DS-100");
        assert!(parent.get("id").is_none() || parent["id"].is_null());
    }

    #[test]
    fn create_issue_resolves_custom_fields_to_jira_ids() {
        let issue = CreateIssue {
            project_id: "PROJ".to_string(),
            summary: "Test".to_string(),
            description: None,
            custom_fields: vec![CustomFieldUpdate::SingleEnum {
                name: "Story Points".to_string(),
                value: "5".to_string(),
            }],
            tags: vec![],
            parent: None,
        };

        let fields = vec![JiraField {
            id: "customfield_10016".to_string(),
            name: "Story Points".to_string(),
            custom: true,
            schema: Some(JiraFieldSchema {
                field_type: "number".to_string(),
                custom: None,
                items: None,
            }),
        }];

        let jira = create_issue_to_jira(&issue, &fields);
        let json = serde_json::to_value(&jira).unwrap();
        assert_eq!(json["fields"]["customfield_10016"], 5.0);
    }

    #[test]
    fn update_issue_resolves_custom_fields_to_jira_ids() {
        let update = UpdateIssue {
            summary: None,
            description: None,
            custom_fields: vec![CustomFieldUpdate::SingleEnum {
                name: "Story Points".to_string(),
                value: "8".to_string(),
            }],
            tags: vec![],
            parent: None,
        };

        let fields = vec![JiraField {
            id: "customfield_10016".to_string(),
            name: "Story Points".to_string(),
            custom: true,
            schema: Some(JiraFieldSchema {
                field_type: "number".to_string(),
                custom: None,
                items: None,
            }),
        }];

        let jira = update_issue_to_jira(&update, &fields);
        let json = serde_json::to_value(&jira).unwrap();
        assert_eq!(json["fields"]["customfield_10016"], 8.0);
    }

    #[test]
    fn resolve_extra_fields_skips_standard_fields() {
        let custom_fields = vec![
            CustomFieldUpdate::SingleEnum {
                name: "Priority".to_string(),
                value: "High".to_string(),
            },
            CustomFieldUpdate::State {
                name: "State".to_string(),
                value: "Backlog".to_string(),
            },
            CustomFieldUpdate::SingleEnum {
                name: "Story Points".to_string(),
                value: "3".to_string(),
            },
        ];

        let jira_fields = vec![
            JiraField {
                id: "customfield_10016".to_string(),
                name: "Story Points".to_string(),
                custom: true,
                schema: Some(JiraFieldSchema {
                    field_type: "number".to_string(),
                    custom: None,
                    items: None,
                }),
            },
            JiraField {
                id: "customfield_11315".to_string(),
                name: "State".to_string(),
                custom: true,
                schema: Some(JiraFieldSchema {
                    field_type: "option".to_string(),
                    custom: None,
                    items: None,
                }),
            },
        ];

        let extra = resolve_extra_fields(&custom_fields, &jira_fields);
        // Priority and State should be skipped (handled by struct/transitions),
        // Story Points should be resolved
        assert!(!extra.contains_key("priority"));
        assert!(!extra.contains_key("customfield_11315")); // "State" custom field should be skipped
        assert_eq!(extra["customfield_10016"], serde_json::json!(3.0));
    }

    #[test]
    fn jira_field_to_core_maps_types_correctly() {
        let number_field = JiraField {
            id: "customfield_10016".to_string(),
            name: "Story Points".to_string(),
            custom: true,
            schema: Some(JiraFieldSchema {
                field_type: "number".to_string(),
                custom: None,
                items: None,
            }),
        };
        let core = jira_field_to_project_custom_field(&number_field);
        assert_eq!(core.id, "customfield_10016");
        assert_eq!(core.name, "Story Points");
        assert_eq!(core.field_type, "number");

        let option_field = JiraField {
            id: "customfield_10020".to_string(),
            name: "Sprint".to_string(),
            custom: true,
            schema: Some(JiraFieldSchema {
                field_type: "array".to_string(),
                custom: None,
                items: Some("string".to_string()),
            }),
        };
        let core = jira_field_to_project_custom_field(&option_field);
        assert_eq!(core.field_type, "enum[*]");
    }

    #[test]
    fn merge_fields_deduplicates_by_name() {
        let standard = get_standard_custom_fields();
        let instance = vec![
            ProjectCustomField {
                id: "priority".to_string(),
                name: "Priority".to_string(), // duplicate
                field_type: "option".to_string(),
                required: false,
                values: vec![],
                state_values: vec![],
            },
            ProjectCustomField {
                id: "customfield_10016".to_string(),
                name: "Story Points".to_string(), // new
                field_type: "number".to_string(),
                required: false,
                values: vec![],
                state_values: vec![],
            },
        ];
        let merged = merge_fields(standard, instance);
        // Should have standard 5 + Story Points = 6
        let sp_count = merged.iter().filter(|f| f.name == "Story Points").count();
        assert_eq!(sp_count, 1);
        let priority_count = merged.iter().filter(|f| f.name == "Priority").count();
        assert_eq!(priority_count, 1);
        // Priority should retain the standard version (with enum values)
        let priority = merged.iter().find(|f| f.name == "Priority").unwrap();
        assert!(!priority.values.is_empty());
    }

    #[test]
    fn flatten_project_statuses_dedupes_by_id() {
        let groups = vec![
            ProjectIssueTypeStatuses {
                id: "10001".to_string(),
                name: "Task".to_string(),
                statuses: vec![
                    ProjectStatus {
                        id: "1".to_string(),
                        name: "Open".to_string(),
                        status_category: None,
                    },
                    ProjectStatus {
                        id: "3".to_string(),
                        name: "In Progress".to_string(),
                        status_category: None,
                    },
                ],
            },
            ProjectIssueTypeStatuses {
                id: "10002".to_string(),
                name: "Bug".to_string(),
                statuses: vec![
                    ProjectStatus {
                        id: "1".to_string(),
                        name: "Open".to_string(),
                        status_category: None,
                    },
                    ProjectStatus {
                        id: "10003".to_string(),
                        name: "Closed".to_string(),
                        status_category: Some(StatusCategory {
                            key: "done".to_string(),
                            name: "Done".to_string(),
                        }),
                    },
                ],
            },
        ];

        let (values, state_values) = flatten_project_statuses(&groups);
        assert_eq!(values, vec!["Open", "In Progress", "Closed"]);
        assert_eq!(state_values.len(), 3);
        assert!(state_values[2].is_resolved); // Closed should be resolved
        assert_eq!(state_values[2].name, "Closed");
    }
}
