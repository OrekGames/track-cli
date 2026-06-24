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
    #[serde(default, with = "chrono::serde::ts_milliseconds_option")]
    pub resolved: Option<DateTime<Utc>>,
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

/// A custom field as projected from the YouTrack issue payload.
///
/// Deserialization is hand-written (not `#[serde(untagged)]`): we buffer each
/// element into a [`serde_json::Value`], read its `$type`, and dispatch to the
/// matching typed variant. A known `$type` whose body fails to parse propagates
/// the error (it does NOT silently degrade to `Unknown`) -- that is the whole
/// point of dispatching by tag rather than using `untagged`. An unrecognized
/// `$type` (or a missing one) becomes `Unknown { name, value: Some(raw) }`,
/// carrying the entire raw element verbatim so no API data is dropped.
#[derive(Debug, Serialize, Clone)]
pub enum CustomField {
    SingleEnum {
        name: String,
        value: Option<EnumValue>,
    },
    State {
        name: String,
        value: Option<StateValue>,
    },
    SingleUser {
        name: String,
        value: Option<UserValue>,
    },
    Text {
        name: String,
        value: Option<TextValue>,
    },
    MultiEnum {
        name: String,
        value: Vec<EnumValue>,
    },
    Unknown {
        name: String,
        value: Option<serde_json::Value>,
    },
}

#[derive(Deserialize)]
struct SingleEnumData {
    name: String,
    #[serde(default)]
    value: Option<EnumValue>,
}

#[derive(Deserialize)]
struct StateData {
    name: String,
    #[serde(default)]
    value: Option<StateValue>,
}

#[derive(Deserialize)]
struct UserData {
    name: String,
    #[serde(default)]
    value: Option<UserValue>,
}

#[derive(Deserialize)]
struct TextData {
    name: String,
    #[serde(default)]
    value: Option<TextValue>,
}

#[derive(Deserialize)]
struct MultiEnumData {
    name: String,
    #[serde(default)]
    value: Vec<EnumValue>,
}

impl<'de> Deserialize<'de> for CustomField {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        use serde_json::Value;

        let raw = Value::deserialize(d)?;
        let ty = raw.get("$type").and_then(Value::as_str);
        let field = match ty {
            Some("SingleEnumIssueCustomField") => {
                let d: SingleEnumData = serde_json::from_value(raw).map_err(D::Error::custom)?;
                CustomField::SingleEnum {
                    name: d.name,
                    value: d.value,
                }
            }
            Some("StateIssueCustomField") => {
                let d: StateData = serde_json::from_value(raw).map_err(D::Error::custom)?;
                CustomField::State {
                    name: d.name,
                    value: d.value,
                }
            }
            Some("SingleUserIssueCustomField") => {
                let d: UserData = serde_json::from_value(raw).map_err(D::Error::custom)?;
                CustomField::SingleUser {
                    name: d.name,
                    value: d.value,
                }
            }
            Some("TextIssueCustomField") => {
                let d: TextData = serde_json::from_value(raw).map_err(D::Error::custom)?;
                CustomField::Text {
                    name: d.name,
                    value: d.value,
                }
            }
            Some("MultiEnumIssueCustomField") => {
                let d: MultiEnumData = serde_json::from_value(raw).map_err(D::Error::custom)?;
                CustomField::MultiEnum {
                    name: d.name,
                    value: d.value,
                }
            }
            _ => {
                let name = raw
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| ty.map(str::to_string))
                    .unwrap_or_else(|| "custom field".to_string());
                CustomField::Unknown {
                    name,
                    value: Some(raw),
                }
            }
        };
        Ok(field)
    }
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

/// Response from POST /api/issuesGetter/count
#[derive(Debug, Deserialize)]
pub struct IssueCountResponse {
    /// Number of matching issues.
    /// Returns -1 if YouTrack has not finished counting yet -- caller must retry.
    pub count: i64,
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
    #[serde(rename = "MultiEnumIssueCustomField")]
    MultiEnum {
        name: String,
        value: Vec<EnumValueInput>,
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

/// Issue attachment
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IssueAttachment {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, with = "chrono::serde::ts_milliseconds_option")]
    pub created: Option<DateTime<Utc>>,
    #[serde(default)]
    pub author: Option<CommentAuthor>,
    #[serde(default)]
    pub comment: Option<IssueCommentRef>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IssueCommentRef {
    pub id: String,
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
    fn custom_field_update_multi_enum_serializes_correctly() {
        let field = CustomFieldUpdate::MultiEnum {
            name: "Platform".to_string(),
            value: vec![
                EnumValueInput {
                    name: "Windows".to_string(),
                },
                EnumValueInput {
                    name: "macOS".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"$type\":\"MultiEnumIssueCustomField\""));
        assert!(json.contains("\"name\":\"Platform\""));
        assert!(json.contains("\"name\":\"Windows\""));
        assert!(json.contains("\"name\":\"macOS\""));
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

    #[test]
    fn custom_field_unknown_type_retains_raw_value() {
        // A field type we don't model (Date) must be captured losslessly as
        // Unknown, keeping the original name and the raw payload verbatim.
        let json = serde_json::json!({
            "$type": "DateIssueCustomField",
            "name": "Due Date",
            "value": 1700000000000i64
        });

        let field: CustomField = serde_json::from_value(json).unwrap();
        match field {
            CustomField::Unknown { name, value } => {
                assert_eq!(name, "Due Date");
                let value = value.expect("raw value must be retained");
                // The whole element is buffered, so the payload is recoverable.
                assert_eq!(
                    value.get("value").and_then(|v| v.as_i64()),
                    Some(1700000000000)
                );
                assert_eq!(
                    value.get("$type").and_then(|v| v.as_str()),
                    Some("DateIssueCustomField")
                );
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn custom_field_malformed_known_type_errors_not_unknown() {
        // A KNOWN $type whose body is malformed (value.name should be a string)
        // must propagate a deserialization error rather than degrading to
        // Unknown -- this is the guarantee vs #[serde(untagged)].
        let json = serde_json::json!({
            "$type": "StateIssueCustomField",
            "name": "State",
            "value": { "name": 123 }
        });

        let result: Result<CustomField, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "malformed known type must error, got {result:?}"
        );
    }

    #[test]
    fn custom_field_known_single_enum_preserves_value() {
        let json = serde_json::json!({
            "$type": "SingleEnumIssueCustomField",
            "name": "Priority",
            "value": { "name": "Major" }
        });

        let field: CustomField = serde_json::from_value(json).unwrap();
        match field {
            CustomField::SingleEnum { name, value } => {
                assert_eq!(name, "Priority");
                assert_eq!(value.map(|v| v.name).as_deref(), Some("Major"));
            }
            other => panic!("expected SingleEnum, got {other:?}"),
        }
    }
}
