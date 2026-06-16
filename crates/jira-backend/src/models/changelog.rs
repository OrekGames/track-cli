use serde::{Deserialize, Serialize};

use super::user::JiraUser;

/// One page of an issue's changelog from
/// `GET /rest/api/3/issue/{key}/changelog`.
///
/// This endpoint is offset-paged (a `PageBean`), unlike the cursor-based issue
/// search.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraChangelogPage {
    #[serde(default)]
    pub start_at: usize,
    #[serde(default)]
    pub max_results: usize,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub is_last: bool,
    #[serde(default)]
    pub values: Vec<JiraChangelogEntry>,
}

/// A single changelog entry: one author making one or more field changes at a
/// single point in time.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraChangelogEntry {
    pub id: Option<String>,
    pub author: Option<JiraUser>,
    /// Creation timestamp (Atlassian datetime string).
    pub created: Option<String>,
    #[serde(default)]
    pub items: Vec<JiraChangelogItem>,
}

/// A single field modification within a changelog entry.
///
/// `from_string`/`to_string` carry human-readable values (status names, user
/// display names); `from`/`to` carry the corresponding internal IDs.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraChangelogItem {
    #[serde(default)]
    pub field: String,
    pub field_id: Option<String>,
    pub fieldtype: Option<String>,
    pub from: Option<String>,
    pub from_string: Option<String>,
    pub to: Option<String>,
    pub to_string: Option<String>,
}
