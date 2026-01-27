use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Common issue representation across all backends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    /// Internal ID
    pub id: String,
    /// Human-readable ID (e.g., "PROJ-123")
    pub id_readable: String,
    /// Issue summary/title
    pub summary: String,
    /// Issue description
    pub description: Option<String>,
    /// Project this issue belongs to
    pub project: ProjectRef,
    /// Custom fields on the issue
    pub custom_fields: Vec<CustomField>,
    /// Tags on the issue
    pub tags: Vec<Tag>,
    /// Creation timestamp
    pub created: DateTime<Utc>,
    /// Last update timestamp
    pub updated: DateTime<Utc>,
}

/// Reference to a project (minimal fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRef {
    pub id: String,
    pub name: Option<String>,
    pub short_name: Option<String>,
}

/// Custom field on an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CustomField {
    SingleEnum {
        name: String,
        value: Option<String>,
    },
    State {
        name: String,
        value: Option<String>,
        is_resolved: bool,
    },
    SingleUser {
        name: String,
        login: Option<String>,
        display_name: Option<String>,
    },
    Text {
        name: String,
        value: Option<String>,
    },
    Unknown {
        name: String,
    },
}

/// Tag on an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
}

/// Full project representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub short_name: String,
    pub description: Option<String>,
}

/// Custom field definition for a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCustomField {
    pub id: String,
    pub name: String,
    pub field_type: String,
    pub required: bool,
    /// Enum values for enum-type fields (Priority, State, Type, etc.)
    #[serde(default)]
    pub values: Vec<String>,
    /// State values with workflow metadata (for state-type fields only)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_values: Vec<StateValueInfo>,
}

/// State value with workflow metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateValueInfo {
    /// State name (e.g., "Open", "In Progress", "Done")
    pub name: String,
    /// Whether this state represents a resolved/completed state
    #[serde(default)]
    pub is_resolved: bool,
    /// Ordinal position in the workflow (lower = earlier in workflow)
    #[serde(default)]
    pub ordinal: i32,
}

/// User that can be assigned to issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub login: Option<String>,
    pub display_name: String,
}

/// Data for creating a new project
#[derive(Debug, Clone)]
pub struct CreateProject {
    /// Human-readable project name
    pub name: String,
    /// Short name / project key (e.g., "PROJ")
    pub short_name: String,
    /// Optional description
    pub description: Option<String>,
}

/// Issue tag (full representation with optional metadata)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTag {
    pub id: String,
    pub name: String,
    pub color: Option<TagColor>,
    pub issues_count: Option<i64>,
}

/// Tag color information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagColor {
    pub id: String,
    pub background: Option<String>,
    pub foreground: Option<String>,
}

/// Issue link type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueLinkType {
    pub id: String,
    pub name: String,
    /// The outward name (e.g., "is parent for")
    pub source_to_target: Option<String>,
    /// The inward name (e.g., "is subtask of")
    pub target_to_source: Option<String>,
    pub directed: bool,
}

/// Link between two issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueLink {
    pub id: String,
    pub direction: Option<String>,
    pub link_type: IssueLinkType,
    pub issues: Vec<LinkedIssue>,
}

/// A linked issue reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedIssue {
    pub id: String,
    pub id_readable: Option<String>,
    pub summary: Option<String>,
}

/// Comment on an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub text: String,
    pub author: Option<CommentAuthor>,
    pub created: Option<DateTime<Utc>>,
}

/// Comment author information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentAuthor {
    pub login: String,
    pub name: Option<String>,
}

/// Data for creating a new issue
#[derive(Debug, Clone)]
pub struct CreateIssue {
    pub project_id: String,
    pub summary: String,
    pub description: Option<String>,
    pub custom_fields: Vec<CustomFieldUpdate>,
    pub tags: Vec<String>,
}

/// Data for updating an issue
#[derive(Debug, Clone, Default)]
pub struct UpdateIssue {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub custom_fields: Vec<CustomFieldUpdate>,
    pub tags: Vec<String>,
}

/// Custom field update value
#[derive(Debug, Clone)]
pub enum CustomFieldUpdate {
    SingleEnum { name: String, value: String },
    State { name: String, value: String },
    SingleUser { name: String, login: String },
}

// ============================================================================
// Knowledge Base / Article Models
// ============================================================================

/// Knowledge base article representation across all backends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    /// Internal ID
    pub id: String,
    /// Human-readable ID (e.g., "PROJ-A-1")
    pub id_readable: String,
    /// Article title
    pub summary: String,
    /// Article content (Markdown)
    pub content: Option<String>,
    /// Project this article belongs to
    pub project: ProjectRef,
    /// Parent article (for hierarchical organization)
    pub parent_article: Option<ArticleRef>,
    /// Whether this article has child articles
    pub has_children: bool,
    /// Tags on the article
    pub tags: Vec<Tag>,
    /// Creation timestamp
    pub created: DateTime<Utc>,
    /// Last update timestamp
    pub updated: DateTime<Utc>,
    /// Article author
    pub reporter: Option<CommentAuthor>,
}

/// Reference to an article (minimal fields for links/hierarchy)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleRef {
    pub id: String,
    pub id_readable: Option<String>,
    pub summary: Option<String>,
}

/// Data for creating a new article
#[derive(Debug, Clone)]
pub struct CreateArticle {
    /// Project ID or shortName
    pub project_id: String,
    /// Article title
    pub summary: String,
    /// Article content (Markdown)
    pub content: Option<String>,
    /// Parent article ID (for creating child articles)
    pub parent_article_id: Option<String>,
    /// Tags to apply
    pub tags: Vec<String>,
}

/// Data for updating an article
#[derive(Debug, Clone, Default)]
pub struct UpdateArticle {
    /// New title (if changing)
    pub summary: Option<String>,
    /// New content (if changing)
    pub content: Option<String>,
    /// Tags to set
    pub tags: Vec<String>,
}

/// Attachment on an article
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleAttachment {
    pub id: String,
    pub name: String,
    pub size: i64,
    pub mime_type: Option<String>,
    pub url: Option<String>,
    pub created: Option<DateTime<Utc>>,
}
