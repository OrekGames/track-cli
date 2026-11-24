//! Error codes and next-step hints.
//!
//! Every failure is reported with a short, stable code and, where the CLI
//! knows one, the command that gets past it. An agent can branch on the code
//! without parsing prose, and a hint saves the round trip of asking how.
//!
//! The exit status groups the codes by what to do next:
//!
//! | Exit | Meaning |
//! |------|---------|
//! | 1 | Any other failure |
//! | 2 | Usage: the command line did not parse (set by clap) |
//! | 3 | Not found |
//! | 4 | Not allowed: authentication, permission, configuration |
//! | 5 | Invalid: the request was refused as given |
//! | 6 | Partial: some items of a batch failed |
//! | 7 | Retryable: the tracker is unreachable, limiting or failing |

use crate::cache::TrackerCache;
use crate::output::Capped;
use colored::Colorize;
use serde::Serialize;
use std::fmt;
use tracker_core::TrackerError;
use tracker_core::validation::{FieldProblem, nearest};

/// Exit status of a failure nothing more specific is known about.
const EXIT_OTHER: u8 = 1;
const EXIT_NOT_FOUND: u8 = 3;
const EXIT_NOT_ALLOWED: u8 = 4;
const EXIT_INVALID: u8 = 5;
const EXIT_PARTIAL: u8 = 6;
const EXIT_RETRYABLE: u8 = 7;

/// The code of a failure the CLI itself detects. Tracker failures carry
/// [`TrackerError::code`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCode {
    /// No usable configuration.
    NotConfigured,
    /// A repository config tried to send the token to a host it chose.
    TokenRefused,
    /// No such `-T` template, or it does not apply to the project.
    TemplateNotFound,
    /// `track guide` has no such topic.
    UnknownTopic,
    /// A field name the project schema does not have.
    UnknownField,
    /// A value the field does not accept.
    InvalidValue,
    /// A field that cannot be set.
    ReadOnlyField,
    /// `issue start` / `complete` asked for a state the project does not have.
    InvalidState,
    /// `--select` paths are not in the command's JSON.
    SelectNoMatch,
    /// Some items of a batch failed.
    PartialFailure,
}

impl CliCode {
    #[cfg(test)]
    const ALL: [CliCode; 10] = [
        CliCode::NotConfigured,
        CliCode::TokenRefused,
        CliCode::TemplateNotFound,
        CliCode::UnknownTopic,
        CliCode::UnknownField,
        CliCode::InvalidValue,
        CliCode::ReadOnlyField,
        CliCode::InvalidState,
        CliCode::SelectNoMatch,
        CliCode::PartialFailure,
    ];

    /// The code as it is reported.
    pub fn as_str(self) -> &'static str {
        match self {
            CliCode::NotConfigured => "not_configured",
            CliCode::TokenRefused => "token_refused",
            CliCode::TemplateNotFound => "template_not_found",
            CliCode::UnknownTopic => "unknown_topic",
            CliCode::UnknownField => "unknown_field",
            CliCode::InvalidValue => "invalid_value",
            CliCode::ReadOnlyField => "read_only_field",
            CliCode::InvalidState => "invalid_state",
            CliCode::SelectNoMatch => "select_no_match",
            CliCode::PartialFailure => "partial_failure",
        }
    }

    fn exit_code(self) -> u8 {
        match self {
            CliCode::TemplateNotFound | CliCode::UnknownTopic => EXIT_NOT_FOUND,
            CliCode::NotConfigured | CliCode::TokenRefused => EXIT_NOT_ALLOWED,
            CliCode::UnknownField
            | CliCode::InvalidValue
            | CliCode::ReadOnlyField
            | CliCode::InvalidState
            | CliCode::SelectNoMatch => EXIT_INVALID,
            CliCode::PartialFailure => EXIT_PARTIAL,
        }
    }
}

/// A failure the CLI itself detected, with the code it reports under.
#[derive(Debug)]
pub struct CliError {
    pub code: CliCode,
    pub message: String,
    pub hint: Option<String>,
}

impl CliError {
    pub fn new(code: CliCode, message: impl Into<String>) -> Self {
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
pub fn coded(code: CliCode, err: anyhow::Error) -> anyhow::Error {
    CliError::new(code, format!("{err:#}")).into()
}

/// Field updates that failed validation.
#[derive(Debug)]
pub struct FieldValidationError {
    pub problems: Vec<FieldProblem>,
}

impl FieldValidationError {
    /// The code of the first problem, the one an agent fixes first.
    fn code(&self) -> CliCode {
        match self.problems.first() {
            Some(FieldProblem::UnknownField { .. }) => CliCode::UnknownField,
            Some(FieldProblem::ReadOnlyField { .. }) => CliCode::ReadOnlyField,
            Some(FieldProblem::InvalidValue { .. }) | None => CliCode::InvalidValue,
        }
    }
}

impl fmt::Display for FieldValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lines: Vec<String> = self.problems.iter().map(|p| p.to_string()).collect();
        write!(f, "{}", lines.join("\n"))
    }
}

impl std::error::Error for FieldValidationError {}

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
    /// The process exit status (see the table at the top of this module).
    #[serde(skip)]
    pub exit_code: u8,
}

/// The typed error that decides how a failure is reported.
enum Cause<'a> {
    Cli(&'a CliError),
    Validation(&'a FieldValidationError),
    Tracker(&'a TrackerError),
}

/// The first typed error in `err`'s chain, if any.
fn classify(err: &anyhow::Error) -> Option<Cause<'_>> {
    err.chain().find_map(|cause| {
        if let Some(cli) = cause.downcast_ref::<CliError>() {
            Some(Cause::Cli(cli))
        } else if let Some(validation) = cause.downcast_ref::<FieldValidationError>() {
            Some(Cause::Validation(validation))
        } else {
            cause.downcast_ref::<TrackerError>().map(Cause::Tracker)
        }
    })
}

impl Cause<'_> {
    fn code(&self) -> &'static str {
        match self {
            Cause::Cli(cli) => cli.code.as_str(),
            Cause::Validation(validation) => validation.code().as_str(),
            Cause::Tracker(tracker) => tracker.code(),
        }
    }

    fn retryable(&self) -> bool {
        matches!(self, Cause::Tracker(tracker) if tracker.retryable())
    }

    fn exit_code(&self) -> u8 {
        match self {
            Cause::Cli(cli) => cli.code.exit_code(),
            Cause::Validation(validation) => validation.code().exit_code(),
            Cause::Tracker(tracker) => tracker_exit_code(tracker),
        }
    }

    /// The next step. May read the cache, so it is only worked out for a
    /// report that is printed.
    fn hint(&self) -> Option<String> {
        match self {
            Cause::Cli(cli) => cli.hint.clone(),
            Cause::Validation(_) => Some(
                "`track prime` lists this project's fields and their values; \
                 --no-validate sends the fields as given."
                    .to_string(),
            ),
            Cause::Tracker(tracker) => tracker_hint(tracker),
        }
    }
}

/// Classify `err` by the first typed error in its chain.
pub fn describe(err: &anyhow::Error) -> ErrorReport {
    let message = format!("{err:#}");
    let cause = classify(err);
    // A certificate that does not chain to a trusted root fails as a network
    // error, but the way past it is a setting, not the connection.
    let hint = crate::output::untrusted_certificate_tip(&message)
        .or_else(|| cause.as_ref().and_then(Cause::hint));
    ErrorReport {
        code: cause.as_ref().map_or(GENERIC_CODE, Cause::code),
        message,
        hint,
        retryable: cause.as_ref().is_some_and(Cause::retryable),
        exit_code: cause.as_ref().map_or(EXIT_OTHER, Cause::exit_code),
    }
}

/// The code alone, for per-item results in batch output. Formats nothing
/// and reads nothing.
pub fn code_of(err: &anyhow::Error) -> &'static str {
    classify(err).map_or(GENERIC_CODE, |cause| cause.code())
}

/// The exit status of a tracker failure, from its [`TrackerError::code`].
fn tracker_exit_code(err: &TrackerError) -> u8 {
    if err.retryable() {
        return EXIT_RETRYABLE;
    }
    match err.code() {
        "issue_not_found" | "project_not_found" | "not_found" => EXIT_NOT_FOUND,
        "auth" | "permission" => EXIT_NOT_ALLOWED,
        "invalid_input" | "unsupported" | "conflict" => EXIT_INVALID,
        // `api`, `parse`, `pagination_stalled`, `io`, and any code added
        // later until it is given a group here.
        _ => EXIT_OTHER,
    }
}

fn tracker_hint(err: &TrackerError) -> Option<String> {
    if let TrackerError::ProjectNotFound(wanted) = err {
        return Some(project_hint(wanted));
    }
    let hint = match err.code() {
        "auth" => "The token was rejected. Check it with `track config test`.",
        "permission" => {
            "The token is valid but may not do this. `track doctor` shows what it can do."
        }
        "rate_limited" => "The tracker is rate limiting. Wait, then run the command again.",
        "server" => "The tracker failed on its side. Run the command again later.",
        "network" => {
            "The tracker could not be reached. Check the URL and connection with `track config test`."
        }
        "issue_not_found" => {
            "Check the id. `track prime` shows this project's key and how its issue ids look."
        }
        "not_found" => {
            "Nothing by that id or name exists, or this token cannot see it. Check the id."
        }
        "unsupported" => "This backend cannot do that. `track doctor` lists what it supports.",
        _ => return None,
    };
    Some(hint.to_string())
}

/// Project keys a hint names before the rest is counted.
const HINT_PROJECTS: usize = 15;

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
    projects_hint(wanted, &keys)
}

fn projects_hint(wanted: &str, keys: &[String]) -> String {
    if keys.is_empty() {
        return "`track p ls` lists the projects this token can see.".to_string();
    }
    let listed = Capped::new(keys.iter().cloned(), HINT_PROJECTS).render("track p ls");
    match nearest(wanted, keys) {
        Some(close) => format!("Did you mean '{close}'? Known projects: {listed}"),
        None => format!("Known projects: {listed}"),
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

/// The stderr text for a failure. `styled` colours the text labels, as far
/// as `--color` and `NO_COLOR` allow; JSON is never coloured.
pub fn render(report: &ErrorReport, format: ErrorFormat, styled: bool) -> String {
    match format {
        ErrorFormat::Json => serde_json::json!({ "error": report }).to_string(),
        ErrorFormat::Text => {
            let (error, code, hint) = if styled {
                (
                    "Error".red().bold().to_string(),
                    "Code".dimmed().to_string(),
                    "Hint".yellow().to_string(),
                )
            } else {
                ("Error".to_string(), "Code".to_string(), "Hint".to_string())
            };
            let mut text = format!("{error}: {}\n{code}: {}", report.message, report.code);
            if let Some(text_hint) = &report.hint {
                text.push_str(&format!("\n{hint}: {text_hint}"));
            }
            text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use std::collections::HashMap;

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
        assert_eq!(report.exit_code, 3);
        assert_eq!(code_of(&err), "issue_not_found");
    }

    #[test]
    fn untyped_errors_get_the_generic_code_and_no_hint() {
        let err = anyhow::anyhow!("something else");
        let report = describe(&err);
        assert_eq!(report.code, GENERIC_CODE);
        assert_eq!(report.hint, None);
        assert_eq!(report.exit_code, 1);
        assert_eq!(code_of(&err), GENERIC_CODE);
    }

    #[test]
    fn cli_errors_carry_their_own_code_and_hint() {
        let err: anyhow::Error = CliError::new(CliCode::TemplateNotFound, "Template 'x' not found")
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
        assert_eq!(report.exit_code, 3);
    }

    #[test]
    fn field_validation_is_coded_by_its_first_problem() {
        let err: anyhow::Error = FieldValidationError {
            problems: vec![
                FieldProblem::ReadOnlyField {
                    name: "Created".to_string(),
                    hint: "set by the tracker".to_string(),
                },
                FieldProblem::UnknownField {
                    name: "Prio".to_string(),
                    available: vec![],
                    suggestion: None,
                },
            ],
        }
        .into();
        let report = describe(&err);
        assert_eq!(report.code, "read_only_field");
        assert_eq!(report.exit_code, 5);
        assert!(report.hint.unwrap().contains("track prime"));
    }

    #[test]
    fn rate_limits_and_outages_are_retryable() {
        for (status, code, retryable, exit) in [
            (429, "rate_limited", true, 7),
            (503, "server", true, 7),
            (403, "permission", false, 4),
            (409, "conflict", false, 5),
            (404, "not_found", false, 3),
            (400, "api", false, 1),
        ] {
            let err = anyhow::Error::from(TrackerError::Api {
                status,
                message: "m".to_string(),
            });
            let report = describe(&err);
            assert_eq!(report.code, code, "{status}");
            assert_eq!(report.retryable, retryable, "{status}");
            assert_eq!(report.exit_code, exit, "{status}");
        }
        let network = describe(&TrackerError::Http("timeout".to_string()).into());
        assert!(network.retryable);
        assert_eq!(network.exit_code, 7);
    }

    #[test]
    fn an_untrusted_certificate_puts_the_fix_in_the_hint() {
        let err: anyhow::Error =
            TrackerError::Http("io: invalid peer certificate: UnknownIssuer".to_string()).into();
        let report = describe(&err);
        let hint = report.hint.as_deref().unwrap();
        assert!(hint.contains("SSL_CERT_FILE"), "{hint}");
        assert!(
            hint.contains("track config set system_certs true"),
            "{hint}"
        );
        // JSON errors stay one object on one line, the tip included.
        let json = render(&report, ErrorFormat::Json, true);
        assert!(!json.contains('\n'), "{json}");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["error"]["hint"], hint);
    }

    #[test]
    fn text_and_json_renderings() {
        let report = ErrorReport {
            code: "unknown_field",
            message: "Unknown field 'Prio'.".to_string(),
            hint: Some("See `track prime`.".to_string()),
            retryable: false,
            exit_code: 5,
        };
        assert_eq!(
            render(&report, ErrorFormat::Text, false),
            "Error: Unknown field 'Prio'.\nCode: unknown_field\nHint: See `track prime`."
        );
        let without_hint = ErrorReport {
            hint: None,
            ..report
        };
        assert_eq!(
            render(&without_hint, ErrorFormat::Text, false),
            "Error: Unknown field 'Prio'.\nCode: unknown_field"
        );
        let json = render(&without_hint, ErrorFormat::Json, true);
        assert!(!json.contains('\n'));
        let json: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"error": {
                "code": "unknown_field",
                "message": "Unknown field 'Prio'.",
                "retryable": false
            }})
        );
    }

    #[test]
    fn project_hints_name_the_closest_key_and_count_the_rest() {
        let keys: Vec<String> = std::iter::once("DEMO".to_string())
            .chain((1..=19).map(|n| format!("K{n:02}")))
            .collect();
        let hint = projects_hint("DEMOO", &keys);
        assert!(
            hint.starts_with("Did you mean 'DEMO'? Known projects: DEMO, K01, K02,"),
            "{hint}"
        );
        assert!(hint.ends_with(", K14, … +5 more (track p ls)"), "{hint}");
        assert_eq!(
            projects_hint("ZZZZZZ", &keys[..2]),
            "Known projects: DEMO, K01"
        );
        assert_eq!(
            projects_hint("X", &[]),
            "`track p ls` lists the projects this token can see."
        );
    }

    /// One of every [`TrackerError`], with each HTTP status `code()` maps.
    fn every_tracker_error() -> Vec<TrackerError> {
        let text = || "x".to_string();
        let mut errors = vec![
            TrackerError::Unauthorized,
            TrackerError::IssueNotFound(text()),
            TrackerError::ProjectNotFound(text()),
            TrackerError::NotFound(text()),
            TrackerError::InvalidInput(text()),
            TrackerError::Unsupported(text()),
            TrackerError::Http(text()),
            TrackerError::Parse(text()),
            TrackerError::PaginationStalled(text()),
            TrackerError::Io(text()),
        ];
        errors.extend(
            [400, 401, 403, 404, 409, 429, 500].map(|status| TrackerError::Api {
                status,
                message: text(),
            }),
        );
        // Fails to compile when TrackerError gains a variant, so that the
        // variant is added above and its code documented.
        for err in &errors {
            match err {
                TrackerError::Unauthorized
                | TrackerError::IssueNotFound(_)
                | TrackerError::ProjectNotFound(_)
                | TrackerError::NotFound(_)
                | TrackerError::InvalidInput(_)
                | TrackerError::Unsupported(_)
                | TrackerError::Api { .. }
                | TrackerError::Http(_)
                | TrackerError::Parse(_)
                | TrackerError::PaginationStalled(_)
                | TrackerError::Io(_) => {}
            }
        }
        errors
    }

    /// Code to exit status, as the `| Exit | Meaning | Codes |` table in a
    /// document lists them.
    fn documented_exit_codes(doc: &str) -> HashMap<String, u8> {
        let mut codes = HashMap::new();
        for line in doc.lines() {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let [_, exit, _, listed, _] = cells.as_slice() else {
                continue;
            };
            let Ok(exit) = exit.parse::<u8>() else {
                continue;
            };
            for code in listed.split(',').filter_map(|code| {
                code.trim()
                    .strip_prefix('`')
                    .and_then(|code| code.strip_suffix('`'))
            }) {
                codes.insert(code.to_string(), exit);
            }
        }
        codes
    }

    #[test]
    fn every_code_is_documented_with_its_exit_status() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for doc in [
            "README.md",
            "docs/agent_guide.md",
            "website/src/content/docs/ai-agents.md",
            "agent-skills/references/output.md",
        ] {
            let text = std::fs::read_to_string(root.join(doc)).unwrap();
            let documented = documented_exit_codes(&text);
            let check = |code: &str, exit: u8| {
                assert_eq!(
                    documented.get(code),
                    Some(&exit),
                    "{doc}: `{code}` should be listed under exit {exit}"
                );
            };
            for code in CliCode::ALL {
                check(code.as_str(), code.exit_code());
            }
            for err in every_tracker_error() {
                check(err.code(), Cause::Tracker(&err).exit_code());
            }
            check(GENERIC_CODE, EXIT_OTHER);
        }
    }
}
