//! Error codes and next-step hints.
//!
//! Every failure is reported with a short, stable code and, where the CLI
//! knows one, the command that gets past it. An agent can branch on the code
//! without parsing prose, and a hint saves the round trip of asking how.

use crate::cache::TrackerCache;
use crate::commands::issue::FieldValidationError;
use serde::Serialize;
use std::fmt;
use tracker_core::TrackerError;
use tracker_core::validation::{FieldProblem, nearest};

/// A failure the CLI itself detected, with the code it reports under.
#[derive(Debug)]
pub struct CliError {
    pub code: &'static str,
    pub message: String,
    pub hint: Option<String>,
}

impl CliError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Give an untyped error a code, keeping its message.
pub fn coded(code: &'static str, err: anyhow::Error) -> anyhow::Error {
    CliError::new(code, format!("{err:#}")).into()
}

/// Code for failures nothing more specific is known about.
pub const GENERIC_CODE: &str = "error";

/// What is reported about a failure.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ErrorReport {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Whether the same command may succeed if simply run again later.
    pub retryable: bool,
}

/// Classify `err` by the first typed error in its chain.
pub fn describe(err: &anyhow::Error) -> ErrorReport {
    let message = format!("{err:#}");
    for cause in err.chain() {
        if let Some(cli) = cause.downcast_ref::<CliError>() {
            return ErrorReport {
                code: cli.code,
                message,
                hint: cli.hint.clone(),
                retryable: false,
            };
        }
        if let Some(validation) = cause.downcast_ref::<FieldValidationError>() {
            let code = match validation.problems.first() {
                Some(FieldProblem::UnknownField { .. }) => "unknown_field",
                Some(FieldProblem::ReadOnlyField { .. }) => "read_only_field",
                Some(FieldProblem::InvalidValue { .. }) | None => "invalid_value",
            };
            return ErrorReport {
                code,
                message,
                hint: Some(
                    "`track prime` lists this project's fields and their values; \
                     --no-validate sends the fields as given."
                        .to_string(),
                ),
                retryable: false,
            };
        }
        if let Some(tracker) = cause.downcast_ref::<TrackerError>() {
            return ErrorReport {
                code: tracker.code(),
                message,
                hint: tracker_hint(tracker),
                retryable: tracker.retryable(),
            };
        }
    }
    ErrorReport {
        code: GENERIC_CODE,
        message,
        hint: None,
        retryable: false,
    }
}

/// The code alone, for per-item results in batch output.
pub fn code_of(err: &anyhow::Error) -> &'static str {
    describe(err).code
}

fn tracker_hint(err: &TrackerError) -> Option<String> {
    let hint = match err.code() {
        "auth" => "The token was rejected. Check it with `track config test`.".to_string(),
        "permission" => {
            "The token is valid but may not do this. `track doctor` shows what it can do."
                .to_string()
        }
        "rate_limited" => "The tracker is rate limiting. Wait, then run the command again.".to_string(),
        "server" => "The tracker failed on its side. Run the command again later.".to_string(),
        "network" => {
            "The tracker could not be reached. Check the URL and connection with `track config test`."
                .to_string()
        }
        "issue_not_found" => {
            "Check the id. `track prime` shows this project's key and how its issue ids look."
                .to_string()
        }
        "not_found" => {
            "Nothing by that id or name exists, or this token cannot see it. Check the id."
                .to_string()
        }
        "project_not_found" => {
            let TrackerError::ProjectNotFound(wanted) = err else {
                return None;
            };
            return Some(project_hint(wanted));
        }
        "unsupported" => "This backend cannot do that. `track doctor` lists what it supports.".to_string(),
        _ => return None,
    };
    Some(hint)
}

/// Name the cached project keys, and the one closest to what was asked for.
fn project_hint(wanted: &str) -> String {
    let keys: Vec<String> = TrackerCache::open_active()
        .and_then(|mut cache| {
            cache.ensure_projects().ok()?;
            Some(
                cache
                    .projects
                    .iter()
                    .map(|p| p.short_name.clone())
                    .collect(),
            )
        })
        .unwrap_or_default();
    if keys.is_empty() {
        return "`track p ls` lists the projects this token can see.".to_string();
    }
    let shown: Vec<&str> = keys.iter().take(15).map(String::as_str).collect();
    let more = if keys.len() > shown.len() {
        format!(", … (+{} more: track p ls)", keys.len() - shown.len())
    } else {
        String::new()
    };
    match nearest(wanted, &keys) {
        Some(close) => format!(
            "Did you mean '{close}'? Known projects: {}{more}",
            shown.join(", ")
        ),
        None => format!("Known projects: {}{more}", shown.join(", ")),
    }
}

/// The process exit code for a failure with `code`.
///
/// | Exit | Meaning |
/// |------|---------|
/// | 1 | Any other failure |
/// | 2 | Usage: the command line did not parse (set by clap) |
/// | 3 | Not found |
/// | 4 | Not allowed: authentication, permission, configuration |
/// | 5 | Invalid: the request was refused as given |
/// | 6 | Partial: some items of a batch failed |
/// | 7 | Retryable: the tracker is unreachable, limiting or failing |
pub fn exit_code(code: &str) -> u8 {
    match code {
        "issue_not_found" | "project_not_found" | "not_found" | "template_not_found"
        | "unknown_topic" => 3,
        "auth" | "permission" | "not_configured" | "token_refused" => 4,
        "unknown_field" | "invalid_value" | "read_only_field" | "invalid_state"
        | "invalid_input" | "select_no_match" | "unsupported" | "conflict" => 5,
        "partial_failure" => 6,
        "network" | "rate_limited" | "server" => 7,
        _ => 1,
    }
}

/// How errors are written to stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum ErrorFormat {
    /// `Error:`, `Code:` and `Hint:` lines
    #[default]
    Text,
    /// One JSON object on one line: {"error": {"code", "message", "hint", "retryable"}}
    Json,
}

/// The stderr lines for a failure, without colour.
pub fn render(report: &ErrorReport, format: ErrorFormat) -> String {
    match format {
        ErrorFormat::Json => serde_json::json!({ "error": report }).to_string(),
        ErrorFormat::Text => {
            let mut text = format!("Error: {}\nCode: {}", report.message, report.code);
            if let Some(hint) = &report.hint {
                text.push_str(&format!("\nHint: {hint}"));
            }
            text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn the_first_typed_cause_decides_the_code() {
        let err = anyhow::Error::from(TrackerError::IssueNotFound("DEMO-9".to_string()))
            .context("Failed to fetch issue 'DEMO-9'");
        let report = describe(&err);
        assert_eq!(report.code, "issue_not_found");
        assert_eq!(
            report.message,
            "Failed to fetch issue 'DEMO-9': Issue not found: DEMO-9"
        );
        assert!(!report.retryable);
        assert!(report.hint.is_some());
    }

    #[test]
    fn untyped_errors_get_the_generic_code_and_no_hint() {
        let report = describe(&anyhow::anyhow!("something else"));
        assert_eq!(report.code, GENERIC_CODE);
        assert_eq!(report.hint, None);
    }

    #[test]
    fn cli_errors_carry_their_own_code_and_hint() {
        let err: anyhow::Error = CliError::new("template_not_found", "Template 'x' not found")
            .with_hint("`track workflow list` shows the templates.")
            .into();
        let err = Err::<(), _>(err).context("outer").unwrap_err();
        let report = describe(&err);
        assert_eq!(report.code, "template_not_found");
        assert_eq!(report.message, "outer: Template 'x' not found");
        assert_eq!(
            report.hint.as_deref(),
            Some("`track workflow list` shows the templates.")
        );
    }

    #[test]
    fn rate_limits_and_outages_are_retryable() {
        for (status, code, retryable) in [
            (429, "rate_limited", true),
            (503, "server", true),
            (403, "permission", false),
            (409, "conflict", false),
            (404, "not_found", false),
            (400, "api", false),
        ] {
            let err = anyhow::Error::from(TrackerError::Api {
                status,
                message: "m".to_string(),
            });
            let report = describe(&err);
            assert_eq!(report.code, code, "{status}");
            assert_eq!(report.retryable, retryable, "{status}");
        }
        assert!(describe(&TrackerError::Http("timeout".to_string()).into()).retryable);
    }

    #[test]
    fn exit_codes_group_failures_by_what_to_do_next() {
        assert_eq!(exit_code("issue_not_found"), 3);
        assert_eq!(exit_code("auth"), 4);
        assert_eq!(exit_code("unknown_field"), 5);
        assert_eq!(exit_code("partial_failure"), 6);
        assert_eq!(exit_code("rate_limited"), 7);
        assert_eq!(exit_code(GENERIC_CODE), 1);
        assert_eq!(exit_code("parse"), 1);
    }

    #[test]
    fn text_and_json_renderings() {
        let report = ErrorReport {
            code: "unknown_field",
            message: "Unknown field 'Prio'.".to_string(),
            hint: Some("See `track prime`.".to_string()),
            retryable: false,
        };
        assert_eq!(
            render(&report, ErrorFormat::Text),
            "Error: Unknown field 'Prio'.\nCode: unknown_field\nHint: See `track prime`."
        );
        let json: serde_json::Value =
            serde_json::from_str(&render(&report, ErrorFormat::Json)).unwrap();
        assert_eq!(json["error"]["code"], "unknown_field");
        assert_eq!(json["error"]["retryable"], false);
        assert!(!render(&report, ErrorFormat::Json).contains('\n'));
    }
}
