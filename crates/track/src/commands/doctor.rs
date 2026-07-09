//! `track doctor` — practical backend capability audit.
//!
//! Goes beyond `config test` (a single `list_projects` call): runs a battery
//! of non-mutating checks per backend and reports per-check statuses inferred
//! from the call results and error taxonomy. No formal `capabilities()` trait
//! is consulted; a capability "exists" if the call works.
//!
//! Checks never mutate remote trackers. `--write-check` only performs local
//! schema validation (can a create/update payload be validated against the
//! project's field schema?); it never sends writes.

use crate::cli::{Backend, Cli, OutputFormat};
use crate::config::Config;
use crate::output::output_json;
use anyhow::{Result, anyhow};
use colored::Colorize;
use serde::Serialize;
use tracker_core::{IssueTracker, KnowledgeBase, TrackerError};

/// Options parsed from the `track doctor` CLI flags.
pub struct DoctorOptions<'a> {
    pub all_backends: bool,
    pub project: Option<&'a str>,
    /// When true, run write validation locally (schema only, no remote writes).
    pub write_check: bool,
    pub strict: bool,
}

/// Status of a single capability check (also used for the per-backend rollup).
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Degraded,
    Failed,
    Skipped,
}

#[derive(Serialize, Debug)]
pub struct CheckResult {
    pub name: &'static str,
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_count: Option<usize>,
}

impl CheckResult {
    fn ok(name: &'static str) -> Self {
        Self {
            name,
            status: CheckStatus::Ok,
            message: None,
            sample_count: None,
        }
    }

    fn skipped(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Skipped,
            message: Some(message.into()),
            sample_count: None,
        }
    }

    fn failed(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Failed,
            message: Some(message.into()),
            sample_count: None,
        }
    }

    fn from_error(name: &'static str, err: &TrackerError) -> Self {
        let (status, message) = classify_error(err);
        Self {
            name,
            status,
            message: Some(message),
            sample_count: None,
        }
    }

    fn with_count(mut self, count: usize) -> Self {
        self.sample_count = Some(count);
        self
    }

    fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

#[derive(Serialize, Debug)]
pub struct ConfigInfo {
    /// Where configuration came from (config file paths, or flags/env).
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_project: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct BackendReport {
    pub backend: String,
    pub status: CheckStatus,
    pub config: ConfigInfo,
    pub checks: Vec<CheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct Summary {
    pub backends_checked: usize,
    pub ok: usize,
    pub degraded: usize,
    pub failed: usize,
}

#[derive(Serialize, Debug)]
pub struct DoctorReport {
    pub summary: Summary,
    pub backends: Vec<BackendReport>,
}

/// Map a backend error to a check status. Never includes tokens: messages are
/// backend error strings, which the error taxonomy keeps secret-free.
///
/// - `Unauthorized` / 401 → failed (bad credentials)
/// - 403 → degraded (valid token, missing scope)
/// - 404 / not-found variants → degraded (reachable, resource missing)
/// - `InvalidInput` "not supported" → skipped (backend lacks the capability)
/// - anything else (network, parse, 5xx, ...) → failed
fn classify_error(err: &TrackerError) -> (CheckStatus, String) {
    match err {
        TrackerError::Unauthorized => (CheckStatus::Failed, err.to_string()),
        TrackerError::Api { status: 401, .. } => (CheckStatus::Failed, err.to_string()),
        TrackerError::Api { status: 403, .. } => (
            CheckStatus::Degraded,
            format!("{} (token likely lacks the required scope)", err),
        ),
        TrackerError::Api { status: 404, .. } => (CheckStatus::Degraded, err.to_string()),
        TrackerError::InvalidInput(msg) if msg.to_lowercase().contains("not supported") => {
            (CheckStatus::Skipped, msg.clone())
        }
        TrackerError::NotFound(_)
        | TrackerError::IssueNotFound(_)
        | TrackerError::ProjectNotFound(_) => (CheckStatus::Degraded, err.to_string()),
        _ => (CheckStatus::Failed, err.to_string()),
    }
}

/// Roll per-check statuses up into a backend status.
///
/// A backend is `failed` only when nothing practical works: a check failed
/// with no usable reads, or the read path itself (`issue_search`/`issue_read`)
/// is broken — e.g. everything 404s under a wrong project id. If read/search
/// workflows are usable despite failing checks, it is merely `degraded`.
fn overall_status(checks: &[CheckResult]) -> CheckStatus {
    let status_of = |name: &str| checks.iter().find(|c| c.name == name).map(|c| c.status);
    let search = status_of("issue_search");
    let read = status_of("issue_read");
    let reads_usable =
        matches!(search, Some(CheckStatus::Ok)) || matches!(read, Some(CheckStatus::Ok));
    // Reads are broken only when a read check actually ran and did not pass;
    // Skipped-only reads (no sample issue, no client) don't count as broken.
    let reads_broken = !reads_usable
        && [search, read]
            .iter()
            .any(|s| matches!(s, Some(CheckStatus::Degraded | CheckStatus::Failed)));

    let any_failed = checks.iter().any(|c| c.status == CheckStatus::Failed);
    let any_degraded = checks.iter().any(|c| c.status == CheckStatus::Degraded);

    if any_failed {
        if reads_usable {
            CheckStatus::Degraded
        } else {
            CheckStatus::Failed
        }
    } else if reads_broken {
        CheckStatus::Failed
    } else if any_degraded {
        CheckStatus::Degraded
    } else {
        CheckStatus::Ok
    }
}

fn recommendation(status: CheckStatus, checks: &[CheckResult]) -> Option<String> {
    match status {
        CheckStatus::Ok | CheckStatus::Skipped => None,
        CheckStatus::Failed => Some(
            "Backend is not usable: fix configuration, credentials, or connectivity \
             (see failed checks) before relying on it."
                .to_string(),
        ),
        CheckStatus::Degraded => {
            let reads_ok = checks
                .iter()
                .any(|c| c.name == "issue_search" && c.status == CheckStatus::Ok)
                || checks
                    .iter()
                    .any(|c| c.name == "issue_read" && c.status == CheckStatus::Ok);
            if reads_ok {
                Some(
                    "Read/search workflows are usable; broader token scope or configuration \
                     is needed for the degraded/failed checks."
                        .to_string(),
                )
            } else {
                Some("Some capability checks did not pass; see check messages.".to_string())
            }
        }
    }
}

/// Remote check names, used to mark everything as skipped when no client can
/// be built (invalid config outside mock mode).
const REMOTE_CHECKS: [&str; 9] = [
    "auth_connectivity",
    "project_resolution",
    "issue_search",
    "issue_read",
    "comments_read",
    "links_read",
    "field_schema",
    "field_admin",
    "articles",
];

pub fn handle_doctor(cli: &Cli, opts: DoctorOptions) -> Result<()> {
    let effective = effective_backend(cli);
    let backends = select_backends(cli, opts.all_backends)?;

    let mut reports = Vec::new();
    for backend in &backends {
        reports.push(audit_backend(
            cli,
            *backend,
            cli_overrides_apply(*backend, effective),
            &opts,
        ));
    }

    let summary = Summary {
        backends_checked: reports.len(),
        ok: reports
            .iter()
            .filter(|r| r.status == CheckStatus::Ok)
            .count(),
        degraded: reports
            .iter()
            .filter(|r| r.status == CheckStatus::Degraded)
            .count(),
        failed: reports
            .iter()
            .filter(|r| r.status == CheckStatus::Failed)
            .count(),
    };
    let report = DoctorReport {
        summary,
        backends: reports,
    };

    match cli.format {
        OutputFormat::Json => output_json(&report)?,
        OutputFormat::Text => print_text(&report),
    }

    if opts.strict {
        let failed_checks: usize = report
            .backends
            .iter()
            .flat_map(|b| &b.checks)
            .filter(|c| c.status == CheckStatus::Failed)
            .count();
        // A backend can roll up failed without any individual check failing
        // (e.g. every remote call 404s, so reads are broken but only
        // degraded); --strict must catch that too.
        let failed_backends: usize = report
            .backends
            .iter()
            .filter(|b| b.status == CheckStatus::Failed)
            .count();
        if failed_checks > 0 || failed_backends > 0 {
            return Err(anyhow!(
                "doctor: {} check(s) and {} backend(s) failed under --strict",
                failed_checks,
                failed_backends
            ));
        }
    }
    Ok(())
}

/// The effective backend: global `-b` flag, then config chain, then default.
fn effective_backend(cli: &Cli) -> Backend {
    cli.backend.unwrap_or_else(crate::config::resolve_backend)
}

/// The global `--url`/`--token` flags belong to the effective backend only;
/// under `--all-backends` they must not leak into other backends' audits
/// (e.g. running the GitLab audit against a YouTrack URL/token).
fn cli_overrides_apply(backend: Backend, effective: Backend) -> bool {
    backend == effective
}

/// Determine which backends to audit: `--all-backends` enumerates every
/// configured backend; otherwise the effective backend (global `-b` flag,
/// then config chain, then default) is audited.
fn select_backends(cli: &Cli, all_backends: bool) -> Result<Vec<Backend>> {
    if all_backends {
        let raw = Config::load_raw(cli.config.clone())?;
        Ok(all_backends_selection(
            raw.configured_backends(),
            cli.backend,
            effective_backend(cli),
        ))
    } else {
        Ok(vec![effective_backend(cli)])
    }
}

/// Union the configured backends with an explicitly requested `-b` backend,
/// deduped in `Backend::ALL` order. CLI-flag-only setups (e.g. --url/--token)
/// have empty raw config; fall back to the effective backend so there is
/// something to audit.
fn all_backends_selection(
    configured: Vec<Backend>,
    explicit: Option<Backend>,
    effective: Backend,
) -> Vec<Backend> {
    let mut selected = configured;
    if let Some(backend) = explicit
        && !selected.contains(&backend)
    {
        selected.push(backend);
    }
    if selected.is_empty() {
        selected.push(effective);
    }
    Backend::ALL
        .into_iter()
        .filter(|b| selected.contains(b))
        .collect()
}

fn config_source(cli: &Cli) -> String {
    if let Some(path) = &cli.config {
        return path.display().to_string();
    }
    let mut parts = Vec::new();
    if let Some(global) = crate::config::global_config_path()
        && global.exists()
    {
        parts.push(global.display().to_string());
    }
    if let Ok(local) = crate::config::local_track_config_path()
        && local.exists()
    {
        parts.push(".track.toml".to_string());
    }
    if parts.is_empty() {
        "flags/env only".to_string()
    } else {
        parts.join(" + ")
    }
}

fn audit_backend(
    cli: &Cli,
    backend: Backend,
    apply_cli_overrides: bool,
    opts: &DoctorOptions,
) -> BackendReport {
    let mut checks: Vec<CheckResult> = Vec::new();

    // Load and collapse config for this backend. Global --url/--token apply
    // only to the effective backend (see cli_overrides_apply).
    let config = match Config::load(cli.config.clone(), backend) {
        Ok(mut c) => {
            if apply_cli_overrides {
                c.merge_with_cli(cli.url.clone(), cli.token.clone());
            }
            Some(c)
        }
        Err(e) => {
            checks.push(CheckResult::failed("config_valid", format!("{:#}", e)));
            None
        }
    };

    let config_info = ConfigInfo {
        source: config_source(cli),
        url: config.as_ref().and_then(|c| c.url.clone()),
        default_project: config.as_ref().and_then(|c| c.default_project.clone()),
    };

    let Some(config) = config else {
        for name in REMOTE_CHECKS {
            checks.push(CheckResult::skipped(name, "config could not be loaded"));
        }
        checks.push(write_check_placeholder(opts.write_check));
        return finish_report(backend, config_info, checks);
    };

    // Check: config_valid (reuses Config::validate, which reports missing keys
    // without leaking secrets).
    let config_valid = match config.validate(backend) {
        Ok(()) => {
            checks.push(CheckResult::ok("config_valid"));
            true
        }
        Err(e) => {
            checks.push(CheckResult::failed("config_valid", e.to_string()));
            false
        }
    };

    // Build the client. Mock mode mirrors normal dispatch: the mock client
    // works without real credentials, so remote checks proceed regardless of
    // config validity there.
    let client = if let Some(mock_dir) = tracker_mock::get_mock_dir() {
        match tracker_mock::MockClient::new(&mock_dir) {
            Ok(c) => Some(crate::BackendClient::Mock(c)),
            Err(e) => {
                checks.push(CheckResult::failed(
                    "auth_connectivity",
                    format!("failed to initialize mock client: {}", e),
                ));
                None
            }
        }
    } else if config_valid {
        match crate::build_client(backend, &config) {
            Ok(c) => Some(c),
            Err(e) => {
                checks.push(CheckResult::failed(
                    "auth_connectivity",
                    format!("failed to build client: {:#}", e),
                ));
                None
            }
        }
    } else {
        None
    };

    let Some(client) = client else {
        for name in REMOTE_CHECKS {
            if !checks.iter().any(|c| c.name == name) {
                checks.push(CheckResult::skipped(
                    name,
                    "no usable client (see config_valid)",
                ));
            }
        }
        checks.push(write_check_placeholder(opts.write_check));
        return finish_report(backend, config_info, checks);
    };

    run_remote_checks(
        client.issue_tracker(),
        client.knowledge_base(),
        backend,
        &config,
        opts,
        &mut checks,
    );

    finish_report(backend, config_info, checks)
}

fn finish_report(backend: Backend, config: ConfigInfo, checks: Vec<CheckResult>) -> BackendReport {
    let status = overall_status(&checks);
    let recommendation = recommendation(status, &checks);
    BackendReport {
        backend: backend.to_string(),
        status,
        config,
        checks,
        recommendation,
    }
}

fn run_remote_checks(
    tracker: &dyn IssueTracker,
    kb: &dyn KnowledgeBase,
    backend: Backend,
    config: &Config,
    opts: &DoctorOptions,
    checks: &mut Vec<CheckResult>,
) {
    // Check: auth_connectivity (same probe as `config test`).
    let projects = match tracker.list_projects() {
        Ok(projects) => {
            checks.push(CheckResult::ok("auth_connectivity").with_count(projects.len()));
            Some(projects)
        }
        Err(e) => {
            checks.push(CheckResult::from_error("auth_connectivity", &e));
            None
        }
    };

    // Check: project_resolution. Uses --project, then the configured default,
    // then falls back to the first accessible project.
    let mut target_project: Option<String> = opts
        .project
        .map(String::from)
        .or_else(|| config.default_project.clone());
    let mut fallback_note = None;
    if target_project.is_none()
        && let Some(projects) = &projects
        && let Some(first) = projects.first()
    {
        target_project = Some(first.short_name.clone());
        fallback_note = Some(format!(
            "no project configured; sampled first accessible project '{}'",
            first.short_name
        ));
    }

    let mut resolved_project_id: Option<String> = None;
    match &target_project {
        Some(project) => match tracker.resolve_project_id(project) {
            Ok(id) => {
                let mut check = CheckResult::ok("project_resolution")
                    .with_message(format!("resolved '{}' to '{}'", project, id));
                if let Some(note) = &fallback_note {
                    check = check
                        .with_message(format!("resolved '{}' to '{}' ({})", project, id, note));
                }
                checks.push(check);
                resolved_project_id = Some(id);
            }
            Err(e) => checks.push(CheckResult::from_error("project_resolution", &e)),
        },
        None => checks.push(CheckResult::skipped(
            "project_resolution",
            "no project configured or accessible (use --project)",
        )),
    }

    // Check: issue_search (limit 1, non-mutating).
    let query = search_probe_query(backend, target_project.as_deref());
    let sample_issue: Option<String> = match tracker.search_issues(&query, 1, 0) {
        Ok(result) => {
            checks.push(CheckResult::ok("issue_search").with_count(result.items.len()));
            result.items.first().map(|issue| {
                if issue.id_readable.is_empty() {
                    issue.id.clone()
                } else {
                    issue.id_readable.clone()
                }
            })
        }
        Err(e) => {
            checks.push(CheckResult::from_error("issue_search", &e));
            None
        }
    };

    // Checks that need a sample issue derived from the search result.
    match &sample_issue {
        Some(id) => {
            match tracker.get_issue(id) {
                Ok(_) => checks
                    .push(CheckResult::ok("issue_read").with_message(format!("read '{}'", id))),
                Err(e) => checks.push(CheckResult::from_error("issue_read", &e)),
            }
            match tracker.get_comments(id) {
                Ok(comments) => {
                    checks.push(CheckResult::ok("comments_read").with_count(comments.len()))
                }
                Err(e) => checks.push(CheckResult::from_error("comments_read", &e)),
            }
            match tracker.get_issue_links(id) {
                Ok(links) => checks.push(CheckResult::ok("links_read").with_count(links.len())),
                Err(e) => checks.push(CheckResult::from_error("links_read", &e)),
            }
        }
        None => {
            for name in ["issue_read", "comments_read", "links_read"] {
                checks.push(CheckResult::skipped(name, "no issue available to sample"));
            }
        }
    }

    // Check: field_schema (project custom fields).
    let project_for_fields = resolved_project_id.as_deref().or(target_project.as_deref());
    let mut schema_fields: Option<usize> = None;
    match project_for_fields {
        Some(project_id) => match tracker.get_project_custom_fields(project_id) {
            Ok(fields) => {
                schema_fields = Some(fields.len());
                checks.push(CheckResult::ok("field_schema").with_count(fields.len()));
            }
            Err(e) => checks.push(CheckResult::from_error("field_schema", &e)),
        },
        None => checks.push(CheckResult::skipped(
            "field_schema",
            "no project available for schema lookup",
        )),
    }

    // Check: field_admin (instance-wide custom field definitions; unsupported
    // backends surface as skipped via the InvalidInput "not supported" default).
    match tracker.list_custom_field_definitions() {
        Ok(defs) => checks.push(CheckResult::ok("field_admin").with_count(defs.len())),
        Err(e) => checks.push(CheckResult::from_error("field_admin", &e)),
    }

    // Check: articles (knowledge base read).
    match kb.list_articles(None, 1, 0) {
        Ok(articles) => checks.push(CheckResult::ok("articles").with_count(articles.len())),
        Err(e) => checks.push(CheckResult::from_error("articles", &e)),
    }

    // Check: write_validation. Never mutates: with --write-check this only
    // verifies that create/update payloads can be validated locally against
    // the project field schema.
    if opts.write_check {
        match schema_fields {
            Some(count) => checks.push(CheckResult::ok("write_validation").with_message(format!(
                "create/update payloads can be validated against the project schema \
                 ({} fields); no remote writes were performed",
                count
            ))),
            None => checks.push(CheckResult::skipped(
                "write_validation",
                "project field schema unavailable; cannot validate write payloads locally",
            )),
        }
    } else {
        checks.push(write_check_placeholder(false));
    }
}

fn write_check_placeholder(write_check: bool) -> CheckResult {
    if write_check {
        CheckResult::skipped(
            "write_validation",
            "write validation requires a usable client",
        )
    } else {
        CheckResult::skipped(
            "write_validation",
            "mutating checks are skipped by default; pass --write-check for local \
             schema validation (no remote writes are ever performed)",
        )
    }
}

/// Cheap per-backend search probe, mirroring the query dialects used by
/// `track context`.
fn search_probe_query(backend: Backend, project: Option<&str>) -> String {
    match (backend, project) {
        (Backend::Jira, Some(p)) => format!("project = {}", p),
        (Backend::Jira, None) => "order by created desc".to_string(),
        // The GitHub client scopes queries to the configured repo itself.
        (Backend::GitHub, _) => "is:issue".to_string(),
        (Backend::GitLab, _) => "state=opened".to_string(),
        (Backend::YouTrack | Backend::Linear, Some(p)) => format!("project: {}", p),
        (Backend::YouTrack | Backend::Linear, None) => String::new(),
    }
}

fn status_label(status: CheckStatus) -> colored::ColoredString {
    match status {
        CheckStatus::Ok => "ok".green(),
        CheckStatus::Degraded => "degraded".yellow(),
        CheckStatus::Failed => "failed".red(),
        CheckStatus::Skipped => "skipped".dimmed(),
    }
}

fn print_text(report: &DoctorReport) {
    println!(
        "{} {} backend(s) checked: {} ok, {} degraded, {} failed",
        "Doctor:".white().bold(),
        report.summary.backends_checked,
        report.summary.ok,
        report.summary.degraded,
        report.summary.failed
    );

    for backend in &report.backends {
        println!();
        println!(
            "{} — {}",
            backend.backend.cyan().bold(),
            status_label(backend.status)
        );
        println!("  {}: {}", "config source".dimmed(), backend.config.source);
        if let Some(url) = &backend.config.url {
            println!("  {}: {}", "url".dimmed(), url);
        }
        for check in &backend.checks {
            let symbol = match check.status {
                CheckStatus::Ok => "✓".green().to_string(),
                CheckStatus::Degraded => "!".yellow().to_string(),
                CheckStatus::Failed => "✗".red().to_string(),
                CheckStatus::Skipped => "-".dimmed().to_string(),
            };
            let mut detail = String::new();
            if let Some(count) = check.sample_count {
                detail.push_str(&format!(" ({} found)", count));
            }
            if let Some(message) = &check.message {
                detail.push_str(&format!(" — {}", message));
            }
            println!(
                "  {} {:<20} {}{}",
                symbol,
                check.name,
                status_label(check.status),
                detail.dimmed()
            );
        }
        if let Some(rec) = &backend.recommendation {
            println!("  {} {}", "→".cyan().bold(), rec);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &'static str, status: CheckStatus) -> CheckResult {
        CheckResult {
            name,
            status,
            message: None,
            sample_count: None,
        }
    }

    #[test]
    fn classify_unauthorized_is_failed() {
        let (status, _) = classify_error(&TrackerError::Unauthorized);
        assert_eq!(status, CheckStatus::Failed);
        let (status, _) = classify_error(&TrackerError::Api {
            status: 401,
            message: "bad credentials".into(),
        });
        assert_eq!(status, CheckStatus::Failed);
    }

    #[test]
    fn classify_forbidden_is_degraded_scope_issue() {
        let (status, message) = classify_error(&TrackerError::Api {
            status: 403,
            message: "insufficient_granular_scope".into(),
        });
        assert_eq!(status, CheckStatus::Degraded);
        assert!(message.contains("scope"), "message: {message}");
    }

    #[test]
    fn classify_not_found_is_degraded() {
        for err in [
            TrackerError::Api {
                status: 404,
                message: "missing".into(),
            },
            TrackerError::NotFound("thing".into()),
            TrackerError::IssueNotFound("X-1".into()),
            TrackerError::ProjectNotFound("X".into()),
        ] {
            let (status, _) = classify_error(&err);
            assert_eq!(status, CheckStatus::Degraded, "for {err:?}");
        }
    }

    #[test]
    fn classify_unsupported_is_skipped() {
        let (status, _) = classify_error(&TrackerError::InvalidInput(
            "Custom field management not supported by this backend".into(),
        ));
        assert_eq!(status, CheckStatus::Skipped);
    }

    #[test]
    fn classify_other_errors_are_failed() {
        for err in [
            TrackerError::Http("connection refused".into()),
            TrackerError::Parse("bad json".into()),
            TrackerError::Api {
                status: 500,
                message: "boom".into(),
            },
            TrackerError::InvalidInput("bad query".into()),
        ] {
            let (status, _) = classify_error(&err);
            assert_eq!(status, CheckStatus::Failed, "for {err:?}");
        }
    }

    #[test]
    fn overall_all_ok_is_ok() {
        let checks = vec![
            check("config_valid", CheckStatus::Ok),
            check("auth_connectivity", CheckStatus::Ok),
            check("field_admin", CheckStatus::Skipped),
        ];
        assert_eq!(overall_status(&checks), CheckStatus::Ok);
    }

    #[test]
    fn overall_failed_check_with_working_reads_is_degraded() {
        // The issue #293 headline case: config test 403s but search/read work.
        let checks = vec![
            check("auth_connectivity", CheckStatus::Failed),
            check("issue_search", CheckStatus::Ok),
            check("issue_read", CheckStatus::Ok),
        ];
        assert_eq!(overall_status(&checks), CheckStatus::Degraded);
    }

    #[test]
    fn overall_failed_check_without_reads_is_failed() {
        let checks = vec![
            check("config_valid", CheckStatus::Failed),
            check("auth_connectivity", CheckStatus::Skipped),
            check("issue_search", CheckStatus::Skipped),
        ];
        assert_eq!(overall_status(&checks), CheckStatus::Failed);
    }

    #[test]
    fn overall_degraded_check_is_degraded() {
        let checks = vec![
            check("auth_connectivity", CheckStatus::Ok),
            check("articles", CheckStatus::Degraded),
        ];
        assert_eq!(overall_status(&checks), CheckStatus::Degraded);
    }

    #[test]
    fn overall_all_degraded_reads_is_failed() {
        // Every remote call 404s (e.g. GitLab with a wrong project_id):
        // nothing practical works, so the backend is failed, not degraded.
        let checks = vec![
            check("config_valid", CheckStatus::Ok),
            check("auth_connectivity", CheckStatus::Degraded),
            check("issue_search", CheckStatus::Degraded),
            check("issue_read", CheckStatus::Degraded),
            check("articles", CheckStatus::Degraded),
        ];
        assert_eq!(overall_status(&checks), CheckStatus::Failed);
    }

    #[test]
    fn overall_scope_limited_token_with_working_reads_is_degraded() {
        // 403s on non-read checks while reads work: scope-limited token.
        let checks = vec![
            check("auth_connectivity", CheckStatus::Degraded),
            check("issue_search", CheckStatus::Ok),
            check("issue_read", CheckStatus::Ok),
            check("field_admin", CheckStatus::Degraded),
        ];
        assert_eq!(overall_status(&checks), CheckStatus::Degraded);
    }

    #[test]
    fn overall_unauthorized_everywhere_is_failed() {
        // 401 on every remote call: bad credentials.
        let checks = vec![
            check("config_valid", CheckStatus::Ok),
            check("auth_connectivity", CheckStatus::Failed),
            check("issue_search", CheckStatus::Failed),
            check("issue_read", CheckStatus::Failed),
        ];
        assert_eq!(overall_status(&checks), CheckStatus::Failed);
    }

    #[test]
    fn overall_skipped_reads_alone_do_not_fail() {
        // Reads that never ran (no sample issue) are not "broken".
        let checks = vec![
            check("auth_connectivity", CheckStatus::Ok),
            check("issue_search", CheckStatus::Skipped),
            check("issue_read", CheckStatus::Skipped),
            check("articles", CheckStatus::Degraded),
        ];
        assert_eq!(overall_status(&checks), CheckStatus::Degraded);
    }

    #[test]
    fn recommendation_only_when_not_ok() {
        assert!(recommendation(CheckStatus::Ok, &[]).is_none());
        assert!(recommendation(CheckStatus::Failed, &[]).is_some());
        let checks = vec![check("issue_search", CheckStatus::Ok)];
        let rec = recommendation(CheckStatus::Degraded, &checks).unwrap();
        assert!(rec.contains("Read/search workflows are usable"), "{rec}");
    }

    #[test]
    fn cli_overrides_apply_only_to_effective_backend() {
        assert!(cli_overrides_apply(Backend::YouTrack, Backend::YouTrack));
        assert!(!cli_overrides_apply(Backend::GitLab, Backend::YouTrack));
    }

    #[test]
    fn all_backends_selection_unions_explicit_backend() {
        // `-b jira` with only [youtrack] configured must still audit jira.
        let selected =
            all_backends_selection(vec![Backend::YouTrack], Some(Backend::Jira), Backend::Jira);
        assert_eq!(selected, vec![Backend::YouTrack, Backend::Jira]);
    }

    #[test]
    fn all_backends_selection_dedups_explicit_backend() {
        let selected = all_backends_selection(
            vec![Backend::YouTrack, Backend::GitLab],
            Some(Backend::GitLab),
            Backend::GitLab,
        );
        assert_eq!(selected, vec![Backend::YouTrack, Backend::GitLab]);
    }

    #[test]
    fn all_backends_selection_falls_back_to_effective() {
        // CLI-flag-only setups have empty raw config.
        let selected = all_backends_selection(vec![], None, Backend::YouTrack);
        assert_eq!(selected, vec![Backend::YouTrack]);
    }

    #[test]
    fn search_probe_queries_per_backend() {
        assert_eq!(
            search_probe_query(Backend::Jira, Some("PROJ")),
            "project = PROJ"
        );
        assert_eq!(
            search_probe_query(Backend::GitHub, Some("PROJ")),
            "is:issue"
        );
        assert_eq!(search_probe_query(Backend::GitLab, None), "state=opened");
        assert_eq!(
            search_probe_query(Backend::YouTrack, Some("PROJ")),
            "project: PROJ"
        );
        assert_eq!(search_probe_query(Backend::Linear, None), "");
    }
}
