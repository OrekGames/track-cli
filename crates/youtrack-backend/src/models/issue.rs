use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub id_readable: String,
    pub summary: String,
    #[serde(default)]
    pub description: Option<String>,
    pub project: ProjectRef,
    #[serde(default)]
    pub custom_fields: Vec<CustomField>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Tag {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TagIdentifier {
    #[serde(rename = "$type")]
    pub entity_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl TagIdentifier {
    pub fn from_name(name: String) -> Self {
        Self {
            entity_type: "IssueTag".to_string(),
            id: None,
            name: Some(name),
        }
    }

    pub fn from_id(id: String) -> Self {
        Self {
            entity_type: "IssueTag".to_string(),
            id: Some(id),
            name: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRef {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "shortName", default)]
    pub short_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "$type")]
pub enum CustomField {
    #[serde(rename = "SingleEnumIssueCustomField")]
    SingleEnum {
        name: String,
        value: Option<EnumValue>,
    },
    #[serde(rename = "StateIssueCustomField")]
    State {
        name: String,
        value: Option<StateValue>,
    },
    #[serde(rename = "SingleUserIssueCustomField")]
    SingleUser {
        name: String,
        value: Option<UserValue>,
    },
    #[serde(rename = "TextIssueCustomField")]
    Text {
        name: String,
        value: Option<TextValue>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EnumValue {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StateValue {
    pub name: String,
    pub is_resolved: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserValue {
    pub login: String,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TextValue {
    pub text: String,
}

/// Custom field update types for write operations
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "$type")]
pub enum CustomFieldUpdate {
    #[serde(rename = "SingleEnumIssueCustomField")]
    SingleEnum {
        name: String,
        value: Option<EnumValueInput>,
    },
    #[serde(rename = "StateIssueCustomField")]
    State {
        name: String,
        value: Option<StateValueInput>,
    },
    #[serde(rename = "SingleUserIssueCustomField")]
    SingleUser {
        name: String,
        value: Option<UserValueInput>,
    },
}

#[derive(Debug, Serialize, Clone)]
pub struct EnumValueInput {
    pub name: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct StateValueInput {
    pub name: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct UserValueInput {
    pub login: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssue {
    pub project: ProjectIdentifier,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_fields: Vec<CustomFieldUpdate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagIdentifier>,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIssue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom_fields: Vec<CustomFieldUpdate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagIdentifier>,
}

#[derive(Serialize, Debug)]
pub struct ProjectIdentifier {
    pub id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IssueList {
    pub issues: Vec<Issue>,
}

/// Issue link type (e.g., "Subtask", "Relates to", etc.)
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IssueLinkType {
    pub id: String,
    pub name: String,
    /// The outward name (e.g., "is parent for")
    #[serde(default)]
    pub source_to_target: Option<String>,
    /// The inward name (e.g., "is subtask of")
    #[serde(default)]
    pub target_to_source: Option<String>,
    #[serde(default)]
    pub directed: bool,
}

/// Issue link between two issues
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IssueLink {
    pub id: String,
    #[serde(default)]
    pub direction: Option<String>,
    pub link_type: IssueLinkType,
    #[serde(default)]
    pub issues: Vec<LinkedIssue>,
}

/// A linked issue reference
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LinkedIssue {
    pub id: String,
    #[serde(default)]
    pub id_readable: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

/// Reference to an issue for linking
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRef {
    pub id_readable: String,
}

/// Issue comment
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IssueComment {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub author: Option<CommentAuthor>,
    #[serde(default, with = "chrono::serde::ts_milliseconds_option")]
    pub created: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CommentAuthor {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// Create a comment on an issue
#[derive(Debug, Serialize)]
pub struct CreateComment {
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_field_update_serializes_with_type_discriminator() {
        let field = CustomFieldUpdate::State {
            name: "State".to_string(),
            value: Some(StateValueInput {
                name: "Open".to_string(),
            }),
        };

        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"$type\":\"StateIssueCustomField\""));
        assert!(json.contains("\"name\":\"State\""));
        assert!(json.contains("\"name\":\"Open\""));
    }

    #[test]
    fn custom_field_update_single_enum_serializes_correctly() {
        let field = CustomFieldUpdate::SingleEnum {
            name: "Priority".to_string(),
            value: Some(EnumValueInput {
                name: "Major".to_string(),
            }),
        };

        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"$type\":\"SingleEnumIssueCustomField\""));
        assert!(json.contains("\"name\":\"Priority\""));
    }

    #[test]
    fn custom_field_update_single_user_serializes_correctly() {
        let field = CustomFieldUpdate::SingleUser {
            name: "Assignee".to_string(),
            value: Some(UserValueInput {
                login: "john.doe".to_string(),
            }),
        };

        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"$type\":\"SingleUserIssueCustomField\""));
        assert!(json.contains("\"login\":\"john.doe\""));
    }

    #[test]
    fn create_issue_serializes_with_custom_fields_and_tags() {
        let create = CreateIssue {
            project: ProjectIdentifier {
                id: "PROJ".to_string(),
            },
            summary: "Test".to_string(),
            description: None,
            custom_fields: vec![CustomFieldUpdate::State {
                name: "State".to_string(),
                value: Some(StateValueInput {
                    name: "Open".to_string(),
                }),
            }],
            tags: vec![TagIdentifier::from_name("bug".to_string())],
        };

        let json = serde_json::to_string(&create).unwrap();
        assert!(json.contains("customFields"));
        assert!(json.contains("tags"));
        assert!(json.contains("\"name\":\"bug\""));
        assert!(json.contains("\"$type\":\"IssueTag\""));
    }

    #[test]
    fn create_issue_omits_empty_custom_fields_and_tags() {
        let create = CreateIssue {
            project: ProjectIdentifier {
                id: "PROJ".to_string(),
            },
            summary: "Test".to_string(),
            description: None,
            custom_fields: vec![],
            tags: vec![],
        };

        let json = serde_json::to_string(&create).unwrap();
        assert!(!json.contains("customFields"));
        assert!(!json.contains("tags"));
    }

    #[test]
    fn tag_identifier_serializes_with_name_only() {
        let tag = TagIdentifier::from_name("urgent".to_string());

        let json = serde_json::to_string(&tag).unwrap();
        assert!(!json.contains("\"id\""));
        assert!(json.contains("\"name\":\"urgent\""));
        assert!(json.contains("\"$type\":\"IssueTag\""));
    }
}
