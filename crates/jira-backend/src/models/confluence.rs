//! Confluence API models for pages, comments, and attachments

use serde::{Deserialize, Serialize};

// ============================================================================
// Page Models
// ============================================================================

/// Confluence page (article equivalent)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluencePage {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub page_type: Option<String>,
    pub status: Option<String>,
    pub space_id: Option<String>,
    pub parent_id: Option<String>,
    pub parent_type: Option<String>,
    pub position: Option<i32>,
    pub author_id: Option<String>,
    pub owner_id: Option<String>,
    pub created_at: Option<String>,
    #[serde(rename = "version")]
    pub version: Option<ConfluenceVersion>,
    pub body: Option<ConfluenceBody>,
    #[serde(rename = "_links")]
    pub links: Option<ConfluenceLinks>,
}

/// Confluence page version info
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceVersion {
    pub number: i32,
    pub message: Option<String>,
    pub minor_edit: Option<bool>,
    pub author_id: Option<String>,
    pub created_at: Option<String>,
}

/// Confluence page body with different representations
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceBody {
    pub storage: Option<ConfluenceBodyValue>,
    pub atlas_doc_format: Option<ConfluenceBodyValue>,
}

/// Body value with representation info
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceBodyValue {
    pub representation: Option<String>,
    pub value: Option<String>,
}

/// Confluence HATEOAS links
#[derive(Debug, Deserialize)]
pub struct ConfluenceLinks {
    #[serde(rename = "webui")]
    pub web_ui: Option<String>,
    #[serde(rename = "editui")]
    pub edit_ui: Option<String>,
    pub tinyui: Option<String>,
    pub base: Option<String>,
}

/// Search result for pages
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluencePageList {
    pub results: Vec<ConfluencePage>,
    #[serde(rename = "_links")]
    pub links: Option<ConfluencePaginationLinks>,
}

/// Pagination links
#[derive(Debug, Deserialize)]
pub struct ConfluencePaginationLinks {
    pub next: Option<String>,
    pub base: Option<String>,
}

// ============================================================================
// Create/Update Page Models
// ============================================================================

/// Request body for creating a page
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfluencePage {
    pub space_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub body: CreateConfluenceBody,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Body for create request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfluenceBody {
    pub representation: String,
    pub value: String,
}

/// Request body for updating a page
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfluencePage {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<CreateConfluenceBody>,
    pub version: UpdateConfluenceVersion,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Version info for update (required for optimistic locking)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfluenceVersion {
    pub number: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ============================================================================
// Space Models
// ============================================================================

/// Confluence space (equivalent to project)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceSpace {
    pub id: String,
    pub key: String,
    pub name: String,
    #[serde(rename = "type")]
    pub space_type: Option<String>,
    pub status: Option<String>,
    pub description: Option<ConfluenceSpaceDescription>,
}

/// Space description
#[derive(Debug, Deserialize)]
pub struct ConfluenceSpaceDescription {
    pub plain: Option<ConfluenceDescriptionValue>,
}

#[derive(Debug, Deserialize)]
pub struct ConfluenceDescriptionValue {
    pub value: Option<String>,
}

/// Space list response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceSpaceList {
    pub results: Vec<ConfluenceSpace>,
    #[serde(rename = "_links")]
    pub links: Option<ConfluencePaginationLinks>,
}

// ============================================================================
// Comment Models
// ============================================================================

/// Confluence footer comment
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceComment {
    pub id: String,
    pub status: Option<String>,
    pub body: Option<ConfluenceBody>,
    pub created_at: Option<String>,
    pub version: Option<ConfluenceVersion>,
    pub page_id: Option<String>,
}

/// Comment list response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceCommentList {
    pub results: Vec<ConfluenceComment>,
    #[serde(rename = "_links")]
    pub links: Option<ConfluencePaginationLinks>,
}

/// Request body for creating a comment
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfluenceComment {
    pub page_id: String,
    pub body: CreateConfluenceBody,
}

// ============================================================================
// Attachment Models
// ============================================================================

/// Confluence attachment
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceAttachment {
    pub id: String,
    pub title: String,
    pub media_type: Option<String>,
    pub media_type_description: Option<String>,
    pub file_size: Option<i64>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub page_id: Option<String>,
    #[serde(rename = "_links")]
    pub links: Option<ConfluenceAttachmentLinks>,
}

/// Attachment links
#[derive(Debug, Deserialize)]
pub struct ConfluenceAttachmentLinks {
    pub download: Option<String>,
}

/// Attachment list response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceAttachmentList {
    pub results: Vec<ConfluenceAttachment>,
    #[serde(rename = "_links")]
    pub links: Option<ConfluencePaginationLinks>,
}

// ============================================================================
// Search Models (v1 API for CQL search)
// ============================================================================

/// Search result from v1 API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceSearchResult {
    pub results: Vec<ConfluenceSearchHit>,
    pub start: Option<i32>,
    pub limit: Option<i32>,
    pub size: Option<i32>,
    pub total_size: Option<i32>,
    #[serde(rename = "_links")]
    pub links: Option<ConfluencePaginationLinks>,
}

/// Individual search hit
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceSearchHit {
    pub content: Option<ConfluenceSearchContent>,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub url: Option<String>,
}

/// Content from search hit
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceSearchContent {
    pub id: String,
    #[serde(rename = "type")]
    pub content_type: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
    pub space: Option<ConfluenceSearchSpace>,
    #[serde(rename = "_links")]
    pub links: Option<ConfluenceLinks>,
}

#[derive(Debug, Deserialize)]
pub struct ConfluenceSearchSpace {
    pub id: Option<i64>,
    pub key: Option<String>,
    pub name: Option<String>,
}

// ============================================================================
// Children Pages Response
// ============================================================================

/// Child pages response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfluenceChildrenResponse {
    pub results: Vec<ConfluencePage>,
    #[serde(rename = "_links")]
    pub links: Option<ConfluencePaginationLinks>,
}
