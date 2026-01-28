//! Manifest parsing and request matching
//!
//! The manifest defines how requests map to response files.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A manifest defines request-to-response mappings for a scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// List of response mappings
    #[serde(default)]
    pub responses: Vec<ResponseMapping>,
}

/// Maps a request to a response file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMapping {
    /// The IssueTracker method name (e.g., "get_issue", "search_issues")
    pub method: String,

    /// Arguments to match (supports wildcards with "*")
    #[serde(default)]
    pub args: HashMap<String, ArgMatcher>,

    /// Response file path (relative to responses/ directory)
    #[serde(default)]
    pub file: Option<String>,

    /// Sequence of response files (for stateful scenarios)
    #[serde(default)]
    pub sequence: Option<Vec<String>>,

    /// HTTP status code to simulate (default: 200)
    #[serde(default = "default_status")]
    pub status: u16,

    /// Conditional matching on request body
    #[serde(default)]
    pub when: Option<ConditionMatcher>,

    /// Delay in milliseconds before returning response (simulates latency)
    #[serde(default)]
    pub delay_ms: u64,
}

fn default_status() -> u16 {
    200
}

/// Argument matcher supporting exact match or wildcard
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgMatcher {
    /// Exact string match
    Exact(String),
    /// Wildcard (matches anything)
    Wildcard,
}

impl ArgMatcher {
    /// Check if this matcher matches the given value
    pub fn matches(&self, value: &str) -> bool {
        match self {
            ArgMatcher::Exact(expected) => {
                if expected == "*" {
                    true
                } else {
                    expected == value
                }
            }
            ArgMatcher::Wildcard => true,
        }
    }
}

/// Condition for matching based on request body or other criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionMatcher {
    /// Request body must contain this string
    #[serde(default)]
    pub body_contains: Option<String>,

    /// Request body must match this JSON structure
    #[serde(default)]
    pub body_json: Option<serde_json::Value>,
}

impl Manifest {
    /// Load a manifest from a TOML file
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| ManifestError::Io(path.to_path_buf(), e))?;

        toml::from_str(&content).map_err(|e| ManifestError::Parse(path.to_path_buf(), e))
    }

    /// Find the best matching response for a request
    ///
    /// Returns the matching ResponseMapping and the call index (for sequence tracking)
    pub fn find_response(
        &self,
        method: &str,
        args: &HashMap<String, String>,
        body: Option<&str>,
        call_counts: &HashMap<String, usize>,
    ) -> Option<(&ResponseMapping, String)> {
        // Generate a unique key for this request (for sequence tracking)
        let request_key = Self::request_key(method, args);

        for mapping in &self.responses {
            if mapping.method != method {
                continue;
            }

            // Check argument matching
            let args_match = mapping
                .args
                .iter()
                .all(|(key, matcher)| args.get(key).map(|v| matcher.matches(v)).unwrap_or(false));

            if !args_match && !mapping.args.is_empty() {
                continue;
            }

            // Check condition matching
            if let Some(condition) = &mapping.when {
                if let Some(body_contains) = &condition.body_contains {
                    if let Some(body) = body {
                        if !body.contains(body_contains) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
            }

            // Determine response file
            let file = if let Some(seq) = &mapping.sequence {
                let count = call_counts.get(&request_key).copied().unwrap_or(0);
                let index = count.min(seq.len().saturating_sub(1));
                seq.get(index).cloned()
            } else {
                mapping.file.clone()
            };

            if let Some(file) = file {
                return Some((mapping, file));
            }
        }

        None
    }

    /// Generate a unique key for a request (method + sorted args)
    pub fn request_key(method: &str, args: &HashMap<String, String>) -> String {
        let mut sorted_args: Vec<_> = args.iter().collect();
        sorted_args.sort_by_key(|(k, _)| *k);

        let args_str: Vec<String> = sorted_args
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        if args_str.is_empty() {
            method.to_string()
        } else {
            format!("{}:{}", method, args_str.join(","))
        }
    }
}

/// Errors that can occur when loading or using a manifest
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("Failed to read manifest file {0}: {1}")]
    Io(std::path::PathBuf, std::io::Error),

    #[error("Failed to parse manifest {0}: {1}")]
    Parse(std::path::PathBuf, toml::de::Error),

    #[error("Response file not found: {0}")]
    ResponseNotFound(std::path::PathBuf),

    #[error("No matching response for method '{method}' with args {args:?}")]
    NoMatch {
        method: String,
        args: HashMap<String, String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arg_matcher_exact() {
        let matcher = ArgMatcher::Exact("DEMO-1".to_string());
        assert!(matcher.matches("DEMO-1"));
        assert!(!matcher.matches("DEMO-2"));
    }

    #[test]
    fn test_arg_matcher_wildcard() {
        let matcher = ArgMatcher::Exact("*".to_string());
        assert!(matcher.matches("anything"));
        assert!(matcher.matches(""));
    }

    #[test]
    fn test_manifest_parse() {
        let toml = r#"
[[responses]]
method = "get_issue"
file = "issue.json"

[responses.args]
id = "DEMO-1"

[[responses]]
method = "list_projects"
file = "projects.json"
"#;

        let manifest: Manifest = toml::from_str(toml).unwrap();
        assert_eq!(manifest.responses.len(), 2);
        assert_eq!(manifest.responses[0].method, "get_issue");
        assert_eq!(manifest.responses[1].method, "list_projects");
    }

    #[test]
    fn test_find_response() {
        let manifest = Manifest {
            responses: vec![
                ResponseMapping {
                    method: "get_issue".to_string(),
                    args: [("id".to_string(), ArgMatcher::Exact("DEMO-1".to_string()))]
                        .into_iter()
                        .collect(),
                    file: Some("demo1.json".to_string()),
                    sequence: None,
                    status: 200,
                    when: None,
                    delay_ms: 0,
                },
                ResponseMapping {
                    method: "get_issue".to_string(),
                    args: [("id".to_string(), ArgMatcher::Exact("*".to_string()))]
                        .into_iter()
                        .collect(),
                    file: Some("fallback.json".to_string()),
                    sequence: None,
                    status: 200,
                    when: None,
                    delay_ms: 0,
                },
            ],
        };

        let args = [("id".to_string(), "DEMO-1".to_string())]
            .into_iter()
            .collect();
        let call_counts = HashMap::new();

        let (mapping, file) = manifest
            .find_response("get_issue", &args, None, &call_counts)
            .unwrap();
        assert_eq!(file, "demo1.json");
        assert_eq!(mapping.method, "get_issue");

        // Test fallback
        let args2 = [("id".to_string(), "OTHER-99".to_string())]
            .into_iter()
            .collect();
        let (_, file2) = manifest
            .find_response("get_issue", &args2, None, &call_counts)
            .unwrap();
        assert_eq!(file2, "fallback.json");
    }
}
