use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub short_name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectList {
    pub projects: Vec<Project>,
}

/// Custom field definition for a project
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCustomField {
    pub id: String,
    pub field: CustomFieldInfo,
    #[serde(default)]
    pub can_be_empty: bool,
    #[serde(default)]
    pub empty_field_text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CustomFieldInfo {
    pub id: String,
    pub name: String,
    pub field_type: Option<FieldType>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FieldType {
    pub id: String,
    #[serde(default)]
    pub presentation: Option<String>,
}

/// Issue tag
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IssueTag {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<TagColor>,
    #[serde(default)]
    pub issues_count: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TagColor {
    pub id: String,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub foreground: Option<String>,
}

/// Request body for creating or updating a tag
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssueTagRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<TagColorRequest>,
}

/// Color for tag create/update requests
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagColorRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground: Option<String>,
}

/// Data for creating a new project via YouTrack API
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProject {
    pub name: String,
    pub short_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// User that can be assigned to issues in a project
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub login: String,
    #[serde(default, rename = "fullName")]
    pub full_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// Extended project custom field with bundle values
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCustomFieldExt {
    pub id: String,
    pub field: CustomFieldInfo,
    #[serde(default)]
    pub can_be_empty: bool,
    #[serde(default)]
    pub empty_field_text: Option<String>,
    #[serde(default)]
    pub bundle: Option<Bundle>,
}

/// Bundle containing enum values for a custom field
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub id: String,
    #[serde(default)]
    pub values: Vec<BundleValue>,
}

/// Value in a bundle (for enum/state fields)
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BundleValue {
    pub id: String,
    pub name: String,
    /// Whether this state value represents a resolved/completed state (state bundles only)
    #[serde(default)]
    pub is_resolved: Option<bool>,
    /// Ordinal position in the workflow (state bundles only, used for transition hints)
    #[serde(default)]
    pub ordinal: Option<i32>,
}
