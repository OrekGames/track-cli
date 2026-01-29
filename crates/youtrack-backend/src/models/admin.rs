//! YouTrack Admin API models for custom field and bundle management

use serde::{Deserialize, Serialize};

// ============================================================================
// Request Models (for creating/updating resources)
// ============================================================================

/// Request body for creating a custom field
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCustomFieldRequest {
    pub name: String,
    pub field_type: FieldTypeRef,
}

/// Reference to a field type by ID
#[derive(Debug, Clone, Serialize)]
pub struct FieldTypeRef {
    pub id: String,
}

/// Request body for creating a bundle
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBundleRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<CreateBundleValueRequest>,
}

/// Request body for creating a bundle value
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBundleValueRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// For state bundles: whether this state represents resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_resolved: Option<bool>,
    /// Position in the workflow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordinal: Option<i32>,
}

/// Request body for attaching a field to a project
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachFieldRequest {
    /// Must be "EnumProjectCustomField", "StateProjectCustomField", etc. based on field type
    #[serde(rename = "$type")]
    pub type_name: String,
    pub field: CustomFieldRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<BundleRef>,
    pub can_be_empty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_field_text: Option<String>,
}

/// Reference to a custom field by ID
#[derive(Debug, Clone, Serialize)]
pub struct CustomFieldRef {
    pub id: String,
}

/// Reference to a bundle by ID
#[derive(Debug, Clone, Serialize)]
pub struct BundleRef {
    #[serde(rename = "$type")]
    pub type_name: String,
    pub id: String,
}

// ============================================================================
// Response Models (for reading resources)
// ============================================================================

/// Custom field definition from the admin API
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomFieldResponse {
    pub id: String,
    pub name: String,
    pub field_type: FieldTypeResponse,
    /// Number of projects using this field
    #[serde(default)]
    pub instances: i32,
}

/// Field type information
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldTypeResponse {
    pub id: String,
    #[serde(default)]
    pub presentation: Option<String>,
}

/// Bundle from the admin API
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleResponse {
    pub id: String,
    pub name: String,
    /// Bundle type (e.g., "EnumBundle", "StateBundle")
    #[serde(rename = "$type")]
    pub bundle_type: String,
    #[serde(default)]
    pub values: Vec<BundleValueResponse>,
}

/// Bundle value from the admin API
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleValueResponse {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// For state bundles: whether this state represents resolution
    #[serde(default)]
    pub is_resolved: Option<bool>,
    /// Position in the workflow
    #[serde(default)]
    pub ordinal: Option<i32>,
}

/// Project custom field attachment response (when attaching a field to a project)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCustomFieldResponse {
    pub id: String,
    pub field: CustomFieldInfoResponse,
    pub can_be_empty: bool,
    #[serde(default)]
    pub empty_field_text: Option<String>,
    #[serde(default)]
    pub bundle: Option<BundleInfoResponse>,
}

/// Custom field info in project attachment response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomFieldInfoResponse {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub field_type: Option<FieldTypeResponse>,
}

/// Bundle info in project attachment response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleInfoResponse {
    pub id: String,
    #[serde(default)]
    pub values: Vec<BundleValueResponse>,
}
