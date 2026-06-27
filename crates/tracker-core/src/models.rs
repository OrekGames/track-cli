use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    /// Resolution timestamp. This is when the backend recorded a resolution,
    /// not a general "is closed" flag — an issue can sit in a resolved/done
    /// state with no resolution date set (e.g. a Jira workflow without a
    /// "Set Resolution" post-function). Use the State custom field's
    /// `is_resolved` to test closedness.
    #[serde(default)]
    pub resolved: Option<DateTime<Utc>>,
}

/// Canonical state, priority, and assignee values extracted from an issue's custom fields.
///
/// Selection rules:
/// - `state`: first [`CustomField::State`] value
/// - `priority`: first [`CustomField::SingleEnum`] whose name is `"priority"` (case-insensitive)
/// - `assignee`: first [`CustomField::SingleUser`] whose name is `"assignee"` (case-insensitive)
#[derive(Debug, Default, Clone, Copy)]
pub struct CommonFields<'a> {
    pub state: Option<&'a str>,
    pub priority: Option<&'a str>,
    pub assignee: Option<&'a str>,
}

impl Issue {
    /// Returns the canonical state, priority, and assignee for display and caching.
    pub fn common_fields(&self) -> CommonFields<'_> {
        let mut fields = CommonFields::default();
        for cf in &self.custom_fields {
            match cf {
                CustomField::State { value, .. } if fields.state.is_none() => {
                    fields.state = value.as_deref();
                }
                CustomField::SingleEnum { name, value }
                    if fields.priority.is_none() && name.eq_ignore_ascii_case("priority") =>
                {
                    fields.priority = value.as_deref();
                }
                CustomField::SingleUser { name, login, .. }
                    if fields.assignee.is_none() && name.eq_ignore_ascii_case("assignee") =>
                {
                    fields.assignee = login.as_deref();
                }
                _ => {}
            }
            if fields.state.is_some() && fields.priority.is_some() && fields.assignee.is_some() {
                break;
            }
        }
        fields
    }

    /// Returns the first workflow state value without scanning unrelated common fields.
    pub fn state_value(&self) -> Option<&str> {
        self.custom_fields.iter().find_map(|cf| match cf {
            CustomField::State { value, .. } => value.as_deref(),
            _ => None,
        })
    }
}

/// Reference to a project (minimal fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRef {
    pub id: String,
    pub name: Option<String>,
    pub short_name: Option<String>,
}

/// Custom field on an issue.
///
/// `custom_fields` is a best-effort-lossless projection of each backend's issue
/// fields. Every backend converter follows one contract:
///
/// 1. Surface a field as the most specific variant it can prove (`State`,
///    `SingleEnum`, `SingleUser`, `Text`, `MultiEnum`).
/// 2. When a value is present but untypeable, emit
///    [`CustomField::Unknown`] with `value: Some(<raw json>)` so the payload
///    round-trips verbatim.
/// 3. Use `Unknown { value: None }` only when a field is known to exist but its
///    value is structurally unretrievable.
/// 4. Never silently drop a field the API returned to the converter, except an
///    enumerated, per-backend noise denylist.
///
/// There is exactly one fallback tag (`Unknown`) across all backends. The
/// externally-tagged JSON encoding is stable and additive: existing variant
/// encodings never change, and `Unknown` only gains an optional `value` key.
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
    MultiEnum {
        name: String,
        values: Vec<String>,
    },
    /// Fallback for a field present on the issue that the backend could not map
    /// to a typed variant above. `value` carries the backend's raw JSON verbatim
    /// (`None` only when the value is structurally unretrievable). See the
    /// projection contract on [`CustomField`].
    Unknown {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<serde_json::Value>,
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

/// Data for creating a new tag/label
#[derive(Debug, Clone)]
pub struct CreateTag {
    /// Tag name
    pub name: String,
    /// Color hex string (e.g., "#d73a4a")
    pub color: Option<String>,
    /// Optional description
    pub description: Option<String>,
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

/// A single field change in an issue's history (changelog / activity timeline).
///
/// Backends model history differently — Jira and YouTrack expose field diffs
/// directly, while GitHub/GitLab expose typed events that must be coerced into
/// this shape. The lowest-common-denominator representation is one field
/// transition: who changed `field` from `from` to `to`, and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueHistoryEvent {
    /// When the change was recorded.
    pub at: DateTime<Utc>,
    /// Who made the change, if known.
    pub author: Option<CommentAuthor>,
    /// Canonical field name that changed (e.g. "status"). See
    /// [`canonical_field_name`].
    pub field: String,
    /// Previous human-readable value, if any.
    pub from: Option<String>,
    /// New human-readable value, if any.
    pub to: Option<String>,
}

/// Canonical name for the workflow status/state field, normalized across
/// backends so a `--field status` filter works regardless of the backend's
/// native terminology (Jira "status", YouTrack "State", GitHub/GitLab close
/// events).
pub const FIELD_STATUS: &str = "status";

/// Normalize a backend's raw history field name to a canonical token.
///
/// Folds known synonyms for the workflow status field onto [`FIELD_STATUS`] so
/// callers can filter portably. All other field names are returned trimmed but
/// otherwise unchanged, preserving their display casing.
pub fn canonical_field_name(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "status" | "state" => FIELD_STATUS.to_string(),
        _ => raw.trim().to_string(),
    }
}

/// Upload request shared by issue and article attachment commands.
#[derive(Debug, Clone)]
pub struct AttachmentUpload {
    pub files: Vec<AttachmentUploadFile>,
    pub comment: Option<String>,
    pub silent: bool,
    pub minor_edit: bool,
}

/// One local file selected for attachment upload.
#[derive(Debug, Clone)]
pub struct AttachmentUploadFile {
    pub path: PathBuf,
    pub name: Option<String>,
    pub mime_type: Option<String>,
}

/// Attachment on an issue or issue comment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueAttachment {
    pub id: String,
    pub name: String,
    pub size: i64,
    pub mime_type: Option<String>,
    pub url: Option<String>,
    pub created: Option<DateTime<Utc>>,
    pub author: Option<CommentAuthor>,
    pub comment_id: Option<String>,
    pub markdown: Option<String>,
}

/// Data for creating a new issue
#[derive(Debug, Clone)]
pub struct CreateIssue {
    pub project_id: String,
    pub summary: String,
    pub description: Option<String>,
    pub custom_fields: Vec<CustomFieldUpdate>,
    pub tags: Vec<String>,
    /// Parent issue ID (e.g., "PROJ-123"). Backend handles natively if supported.
    pub parent: Option<String>,
}

/// Data for updating an issue
#[derive(Debug, Clone, Default)]
pub struct UpdateIssue {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub custom_fields: Vec<CustomFieldUpdate>,
    pub tags: Vec<String>,
    /// Parent issue ID (e.g., "PROJ-123"). Backend handles natively if supported.
    pub parent: Option<String>,
}

/// Custom field update value
#[derive(Debug, Clone)]
pub enum CustomFieldUpdate {
    SingleEnum { name: String, value: String },
    MultiEnum { name: String, values: Vec<String> },
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

// ============================================================================
// Custom Field Admin Models
// ============================================================================

/// Type of custom field
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustomFieldType {
    /// Single enum value selection
    SingleEnum,
    /// Multiple enum value selection
    MultiEnum,
    /// State field with workflow support
    State,
    /// Free-form text field
    Text,
    /// Date field
    Date,
    /// Integer number field
    Integer,
    /// Floating point number field
    Float,
    /// Time period field
    Period,
}

impl CustomFieldType {
    /// Convert to YouTrack field type ID
    pub fn to_youtrack_id(&self) -> &'static str {
        match self {
            CustomFieldType::SingleEnum => "enum[1]",
            CustomFieldType::MultiEnum => "enum[*]",
            CustomFieldType::State => "state[1]",
            CustomFieldType::Text => "text[1]",
            CustomFieldType::Date => "date",
            CustomFieldType::Integer => "integer",
            CustomFieldType::Float => "float",
            CustomFieldType::Period => "period",
        }
    }

    /// Convert to YouTrack project custom field $type name
    pub fn to_project_custom_field_type(&self) -> &'static str {
        match self {
            CustomFieldType::SingleEnum => "EnumProjectCustomField",
            CustomFieldType::MultiEnum => "EnumProjectCustomField",
            CustomFieldType::State => "StateProjectCustomField",
            CustomFieldType::Text => "TextProjectCustomField",
            CustomFieldType::Date => "DateProjectCustomField",
            CustomFieldType::Integer => "SimpleProjectCustomField",
            CustomFieldType::Float => "SimpleProjectCustomField",
            CustomFieldType::Period => "PeriodProjectCustomField",
        }
    }

    /// Parse from string representation
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "enum" | "single-enum" | "singleenum" => Some(CustomFieldType::SingleEnum),
            "multi-enum" | "multienum" => Some(CustomFieldType::MultiEnum),
            "state" => Some(CustomFieldType::State),
            "text" => Some(CustomFieldType::Text),
            "date" => Some(CustomFieldType::Date),
            "integer" | "int" => Some(CustomFieldType::Integer),
            "float" => Some(CustomFieldType::Float),
            "period" => Some(CustomFieldType::Period),
            _ => None,
        }
    }
}

impl std::fmt::Display for CustomFieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CustomFieldType::SingleEnum => "enum",
            CustomFieldType::MultiEnum => "multi-enum",
            CustomFieldType::State => "state",
            CustomFieldType::Text => "text",
            CustomFieldType::Date => "date",
            CustomFieldType::Integer => "integer",
            CustomFieldType::Float => "float",
            CustomFieldType::Period => "period",
        };
        write!(f, "{}", s)
    }
}

/// Type of bundle for storing field values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundleType {
    /// Enumeration values
    Enum,
    /// State values with workflow support
    State,
    /// Owned field bundle (for assignee-like fields)
    OwnedField,
    /// Version bundle
    Version,
    /// Build bundle
    Build,
}

impl BundleType {
    /// Convert to YouTrack API path segment
    pub fn to_api_path(&self) -> &'static str {
        match self {
            BundleType::Enum => "enum",
            BundleType::State => "state",
            BundleType::OwnedField => "ownedField",
            BundleType::Version => "version",
            BundleType::Build => "build",
        }
    }

    /// Convert to YouTrack $type name for API references
    pub fn to_youtrack_type(&self) -> &'static str {
        match self {
            BundleType::Enum => "EnumBundle",
            BundleType::State => "StateBundle",
            BundleType::OwnedField => "OwnedFieldBundle",
            BundleType::Version => "VersionBundle",
            BundleType::Build => "BuildBundle",
        }
    }

    /// Parse from string representation
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "enum" => Some(BundleType::Enum),
            "state" => Some(BundleType::State),
            "ownedfield" | "owned" | "owned-field" => Some(BundleType::OwnedField),
            "version" => Some(BundleType::Version),
            "build" => Some(BundleType::Build),
            _ => None,
        }
    }
}

impl std::fmt::Display for BundleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BundleType::Enum => "enum",
            BundleType::State => "state",
            BundleType::OwnedField => "ownedField",
            BundleType::Version => "version",
            BundleType::Build => "build",
        };
        write!(f, "{}", s)
    }
}

/// Data for creating a new custom field definition
#[derive(Debug, Clone)]
pub struct CreateCustomField {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: CustomFieldType,
}

/// Data for creating a new bundle
#[derive(Debug, Clone)]
pub struct CreateBundle {
    /// Bundle name
    pub name: String,
    /// Bundle type
    pub bundle_type: BundleType,
    /// Initial values for the bundle
    pub values: Vec<CreateBundleValue>,
}

/// Data for creating a bundle value
#[derive(Debug, Clone)]
pub struct CreateBundleValue {
    /// Value name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Whether this value represents a resolved state (for state bundles)
    pub is_resolved: Option<bool>,
    /// Ordinal position in workflow (for state bundles)
    pub ordinal: Option<i32>,
}

/// Data for attaching a custom field to a project
#[derive(Debug, Clone)]
pub struct AttachFieldToProject {
    /// Field ID to attach
    pub field_id: String,
    /// Bundle ID (required for enum/state fields)
    pub bundle_id: Option<String>,
    /// Whether the field can be empty
    pub can_be_empty: bool,
    /// Text to display when field is empty
    pub empty_field_text: Option<String>,
    /// Field type for the $type discriminator (e.g., CustomFieldType::SingleEnum)
    pub field_type: Option<CustomFieldType>,
    /// Bundle type (required if bundle_id is set)
    pub bundle_type: Option<BundleType>,
}

/// Global custom field definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomFieldDefinition {
    /// Field ID
    pub id: String,
    /// Field name
    pub name: String,
    /// Field type string (e.g., "enum[1]", "state[1]")
    pub field_type: String,
    /// Number of projects using this field
    #[serde(default)]
    pub instances_count: i32,
}

/// Bundle definition (collection of values for enum/state fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleDefinition {
    /// Bundle ID
    pub id: String,
    /// Bundle name
    pub name: String,
    /// Bundle type
    pub bundle_type: String,
    /// Values in the bundle
    pub values: Vec<BundleValueDefinition>,
}

/// Single value in a bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleValueDefinition {
    /// Value ID
    pub id: String,
    /// Value name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Whether this value represents a resolved state (for state bundles)
    pub is_resolved: Option<bool>,
    /// Ordinal position in workflow
    pub ordinal: Option<i32>,
}

/// Wrapper for paginated search results that optionally includes a total count.
///
/// Backends that know the total (Jira, GitHub, GitLab) populate `total`.
/// YouTrack chains a count call before the search to get it.
/// `total` is `None` if the backend cannot report a count (e.g., count timed out).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult<T> {
    /// The items returned for this page
    pub items: Vec<T>,
    /// Total number of matching results across all pages, if known.
    pub total: Option<u64>,
}

impl<T> SearchResult<T> {
    /// Create a SearchResult with no total (unknown)
    pub fn from_items(items: Vec<T>) -> Self {
        Self { items, total: None }
    }

    /// Create a SearchResult with a known total
    pub fn with_total(items: Vec<T>, total: u64) -> Self {
        Self {
            items,
            total: Some(total),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn issue_with_custom_fields(custom_fields: Vec<CustomField>) -> Issue {
        Issue {
            id: "1".into(),
            id_readable: "PROJ-1".into(),
            summary: "Test".into(),
            description: None,
            project: ProjectRef {
                id: "PROJ".into(),
                name: None,
                short_name: Some("PROJ".into()),
            },
            custom_fields,
            tags: vec![],
            created: Utc::now(),
            updated: Utc::now(),
            resolved: None,
        }
    }

    #[test]
    fn common_fields_extracts_all_three() {
        let issue = issue_with_custom_fields(vec![
            CustomField::State {
                name: "State".into(),
                value: Some("Open".into()),
                is_resolved: false,
            },
            CustomField::SingleEnum {
                name: "Priority".into(),
                value: Some("High".into()),
            },
            CustomField::SingleUser {
                name: "Assignee".into(),
                login: Some("alice".into()),
                display_name: None,
            },
        ]);

        let common = issue.common_fields();
        assert_eq!(common.state, Some("Open"));
        assert_eq!(common.priority, Some("High"));
        assert_eq!(common.assignee, Some("alice"));
    }

    #[test]
    fn common_fields_priority_name_is_case_insensitive() {
        let issue = issue_with_custom_fields(vec![CustomField::SingleEnum {
            name: "PRIORITY".into(),
            value: Some("Low".into()),
        }]);

        assert_eq!(issue.common_fields().priority, Some("Low"));
    }

    #[test]
    fn common_fields_first_state_wins() {
        let issue = issue_with_custom_fields(vec![
            CustomField::State {
                name: "Stage".into(),
                value: Some("Done".into()),
                is_resolved: true,
            },
            CustomField::State {
                name: "State".into(),
                value: Some("Open".into()),
                is_resolved: false,
            },
        ]);

        assert_eq!(issue.common_fields().state, Some("Done"));
    }

    #[test]
    fn common_fields_stops_after_all_three_found() {
        let issue = issue_with_custom_fields(vec![
            CustomField::State {
                name: "State".into(),
                value: Some("Open".into()),
                is_resolved: false,
            },
            CustomField::SingleEnum {
                name: "Priority".into(),
                value: Some("High".into()),
            },
            CustomField::SingleUser {
                name: "Assignee".into(),
                login: Some("alice".into()),
                display_name: None,
            },
            CustomField::SingleUser {
                name: "Reviewer".into(),
                login: Some("bob".into()),
                display_name: None,
            },
        ]);

        assert_eq!(issue.common_fields().assignee, Some("alice"));
    }

    #[test]
    fn common_fields_skips_reviewer_before_assignee() {
        let issue = issue_with_custom_fields(vec![
            CustomField::SingleUser {
                name: "Reviewer".into(),
                login: Some("bob".into()),
                display_name: None,
            },
            CustomField::SingleUser {
                name: "aSsIgNeE".into(),
                login: Some("alice".into()),
                display_name: None,
            },
        ]);

        assert_eq!(issue.common_fields().assignee, Some("alice"));
    }

    #[test]
    fn common_fields_returns_no_assignee_for_other_user_fields() {
        let issue = issue_with_custom_fields(vec![CustomField::SingleUser {
            name: "Reviewer".into(),
            login: Some("bob".into()),
            display_name: None,
        }]);

        assert_eq!(issue.common_fields().assignee, None);
    }

    #[test]
    fn state_value_returns_first_state_without_requiring_common_fields() {
        let issue = issue_with_custom_fields(vec![
            CustomField::SingleUser {
                name: "Assignee".into(),
                login: Some("alice".into()),
                display_name: None,
            },
            CustomField::State {
                name: "Status".into(),
                value: Some("In Progress".into()),
                is_resolved: false,
            },
        ]);

        assert_eq!(issue.state_value(), Some("In Progress"));
    }

    #[test]
    fn search_result_from_items() {
        let result = SearchResult::from_items(vec![1, 2, 3]);
        assert_eq!(result.items, vec![1, 2, 3]);
        assert_eq!(result.total, None);
    }

    #[test]
    fn search_result_with_total() {
        let result = SearchResult::with_total(vec![1, 2, 3], 100);
        assert_eq!(result.items, vec![1, 2, 3]);
        assert_eq!(result.total, Some(100));
    }
}
