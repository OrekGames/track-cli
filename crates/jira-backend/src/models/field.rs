use serde::Deserialize;

/// A field definition from Jira's `/rest/api/3/field` endpoint.
/// Includes both system fields and instance-level custom fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraField {
    /// Field ID (e.g., "customfield_10016", "priority", "status")
    pub id: String,
    /// Human-readable name (e.g., "Story Points", "Priority")
    pub name: String,
    /// Whether this is a custom field
    #[serde(default)]
    pub custom: bool,
    /// Schema describing the field type
    pub schema: Option<JiraFieldSchema>,
}

/// Schema metadata for a Jira field
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraFieldSchema {
    /// Field type (e.g., "number", "string", "array", "option")
    #[serde(rename = "type")]
    pub field_type: String,
    /// For custom fields, the custom type (e.g., "com.atlassian.jira.plugin.system.customfieldtypes:float")
    pub custom: Option<String>,
    /// For array types, the item type
    pub items: Option<String>,
}
