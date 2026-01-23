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
