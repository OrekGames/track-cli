use serde::{Deserialize, Deserializer, Serialize};

use super::issue::CommentAuthor;

/// One entry from `GET /api/issues/{id}/activities`.
///
/// We request only the `CustomFieldCategory`, so each activity describes a
/// single field transition: who changed `field`, when (`timestamp`), and the
/// values that were `removed` (the old value) and `added` (the new value).
///
/// YouTrack's `added`/`removed` payloads are deliberately polymorphic — the
/// API returns either a single object or a list depending on field arity — so
/// they are normalized to `Vec<ActivityValue>` here (see [`one_or_many`]).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YouTrackActivity {
    #[serde(default)]
    pub id: Option<String>,
    /// Event time as a Unix timestamp in milliseconds (UTC).
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub author: Option<CommentAuthor>,
    #[serde(default)]
    pub field: Option<ActivityField>,
    /// New value(s) of the field after the change.
    #[serde(default, deserialize_with = "one_or_many")]
    pub added: Vec<ActivityValue>,
    /// Previous value(s) of the field before the change.
    #[serde(default, deserialize_with = "one_or_many")]
    pub removed: Vec<ActivityValue>,
}

/// The field that changed. `name` is the underlying custom-field name; some
/// instances populate only the human-readable `presentation`, so both are
/// optional and the converter falls back from one to the other.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivityField {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub presentation: Option<String>,
}

/// One added/removed value. The concrete shape varies by field type
/// (enum, state, user, text, ...), so every plausible label is captured
/// defensively and the converter picks the first non-empty one.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivityValue {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub presentation: Option<String>,
}

/// Deserialize a field that YouTrack may emit as `null`, a single object, or a
/// list of objects into a flat `Vec`. A `null` or absent value yields an empty
/// vec; a lone object yields a single-element vec.
fn one_or_many<'de, D>(deserializer: D) -> Result<Vec<ActivityValue>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Many(Vec<ActivityValue>),
        One(ActivityValue),
    }

    match Option::<OneOrMany>::deserialize(deserializer)? {
        None => Ok(Vec::new()),
        Some(OneOrMany::Many(v)) => Ok(v),
        Some(OneOrMany::One(v)) => Ok(vec![v]),
    }
}
