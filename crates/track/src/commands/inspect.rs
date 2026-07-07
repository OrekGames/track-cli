//! `track issue inspect` — fetch full context for many issues in one invocation.
//!
//! Composes existing `IssueTracker` methods (`get_issue`, `search_issues`,
//! `get_all_comments`, `get_issue_links`, `get_issue_history`) into a single
//! multi-issue report with per-issue success/failure results. A failing issue
//! is recorded in `errors` instead of aborting the run; `--strict` makes the
//! command exit non-zero after reporting all results.

use crate::cli::OutputFormat;
use crate::output::{output_json, output_progress};
use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use tracker_core::{Issue, IssueLink, IssueTracker, get_max_results, unicode_eq_ignore_case};

/// Arguments for `issue inspect`.
pub(crate) struct InspectArgs<'a> {
    pub ids: &'a [String],
    pub ids_file: Option<&'a Path>,
    pub query: Option<&'a str>,
    pub template: Option<&'a str>,
    pub project: Option<&'a str>,
    pub limit: usize,
    pub skip: usize,
    pub all: bool,
    pub include: &'a [String],
    pub jsonl: bool,
    pub strict: bool,
}

/// Which expensive context to fetch per issue. Default is all-off (fast).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Includes {
    comments: bool,
    links: bool,
    subtasks: bool,
    history: bool,
}

/// A structured warning attached to one issue's result (e.g. an include the
/// backend does not support). Warnings never fail the issue.
#[derive(Debug, Serialize)]
struct IncludeWarning {
    include: &'static str,
    message: String,
}

/// A per-issue failure entry in the top-level `errors` array.
#[derive(Debug, Serialize)]
struct InspectError {
    id: String,
    error: String,
}

/// Result of inspecting one issue: a flattened issue object (success) or an
/// error entry (failure), kept in input order for JSONL output.
enum InspectOutcome {
    Success(Map<String, Value>),
    Failure(InspectError),
}

/// Borrowing JSONL success line: a `success` flag plus the flattened issue
/// object, serialized without cloning the (potentially large) issue map.
#[derive(Serialize)]
struct JsonlSuccess<'a> {
    success: bool,
    #[serde(flatten)]
    issue: &'a Map<String, Value>,
}

pub(crate) fn handle_inspect(
    client: &dyn IssueTracker,
    args: &InspectArgs,
    format: OutputFormat,
    default_project: Option<&str>,
    link_mappings: &HashMap<String, String>,
) -> Result<()> {
    let includes = parse_includes(args.include)?;

    let mut succeeded = 0usize;
    let mut failed = 0usize;
    // Outcomes buffered for the JSON/text reports; --jsonl instead streams
    // each result line as soon as its issue is processed and buffers nothing.
    let mut outcomes: Vec<InspectOutcome> = Vec::new();
    let mut record = |outcome: InspectOutcome| -> Result<()> {
        match &outcome {
            InspectOutcome::Success(_) => succeeded += 1,
            InspectOutcome::Failure(_) => failed += 1,
        }
        if args.jsonl {
            output_jsonl_line(&outcome)
        } else {
            outcomes.push(outcome);
            Ok(())
        }
    };

    // Backend-reported match count in query mode; may exceed the fetched page.
    let mut query_total: Option<u64> = None;

    // Resolve the issue set. Query/template mode returns full issues from
    // search (no per-issue get_issue round-trip); ID mode fetches each issue.
    if args.query.is_some() || args.template.is_some() {
        let query = super::issue::resolve_search_query(
            args.query,
            args.template,
            args.project,
            default_project,
        )?;
        let issues = if args.all {
            let res = client
                .search_all_issues(&query, get_max_results())
                .context("Failed to search issues (pagination)")?;
            output_progress(&format!("Fetched {} issues", res.len()), format);
            res
        } else {
            let result = client
                .search_issues(&query, args.limit, args.skip)
                .context("Failed to search issues")?;
            query_total = result.total;
            result.items
        };
        for issue in issues {
            let id = issue.id_readable.clone();
            record(inspect_issue(client, &id, issue, includes, link_mappings))?;
        }
    } else {
        let ids = collect_ids(args.ids, args.ids_file)?;
        if ids.is_empty() {
            return Err(anyhow!(
                "No issue IDs given. Provide positional IDs, --ids <path>, --query, or --template"
            ));
        }
        for id in ids {
            let outcome = match client.get_issue(&id) {
                Ok(issue) => inspect_issue(client, &id, issue, includes, link_mappings),
                Err(e) => InspectOutcome::Failure(InspectError {
                    id,
                    error: e.to_string(),
                }),
            };
            record(outcome)?;
        }
    }

    let total = succeeded + failed;

    if !args.jsonl {
        match format {
            OutputFormat::Json => {
                output_report_json(&outcomes, total, succeeded, failed, query_total)?
            }
            OutputFormat::Text => {
                output_report_text(&outcomes, total, succeeded, failed, query_total)
            }
        }
    }

    if args.strict && failed > 0 {
        return Err(anyhow!(
            "{} of {} issues failed inspection (--strict)",
            failed,
            total
        ));
    }
    Ok(())
}

/// Fetch the requested context for one already-retrieved issue and flatten it
/// into a JSON object: issue fields at the top level, plus `comments`,
/// `links`, `subtasks`, `history`, and `warnings` keys as applicable.
fn inspect_issue(
    client: &dyn IssueTracker,
    id: &str,
    issue: Issue,
    includes: Includes,
    link_mappings: &HashMap<String, String>,
) -> InspectOutcome {
    let mut warnings: Vec<IncludeWarning> = Vec::new();

    let mut obj = match serde_json::to_value(&issue) {
        Ok(Value::Object(map)) => map,
        Ok(_) | Err(_) => {
            return InspectOutcome::Failure(InspectError {
                id: id.to_string(),
                error: "Failed to serialize issue".to_string(),
            });
        }
    };

    if includes.comments {
        match client.get_all_comments(id, get_max_results()) {
            Ok(comments) => {
                obj.insert(
                    "comments".to_string(),
                    serde_json::to_value(comments).unwrap_or(Value::Null),
                );
            }
            Err(e) => warnings.push(IncludeWarning {
                include: "comments",
                message: e.to_string(),
            }),
        }
    }

    if includes.links || includes.subtasks {
        match client.get_issue_links(id) {
            Ok(links) => {
                if includes.subtasks {
                    let subtasks: Vec<&IssueLink> = links
                        .iter()
                        .filter(|l| is_subtask_link(l, link_mappings))
                        .collect();
                    obj.insert(
                        "subtasks".to_string(),
                        serde_json::to_value(subtasks).unwrap_or(Value::Null),
                    );
                }
                if includes.links {
                    obj.insert(
                        "links".to_string(),
                        serde_json::to_value(links).unwrap_or(Value::Null),
                    );
                }
            }
            Err(e) => {
                if includes.links {
                    warnings.push(IncludeWarning {
                        include: "links",
                        message: e.to_string(),
                    });
                }
                if includes.subtasks {
                    warnings.push(IncludeWarning {
                        include: "subtasks",
                        message: e.to_string(),
                    });
                }
            }
        }
    }

    if includes.history {
        match client.get_issue_history(id) {
            Ok(events) => {
                obj.insert(
                    "history".to_string(),
                    serde_json::to_value(events).unwrap_or(Value::Null),
                );
            }
            Err(e) => warnings.push(IncludeWarning {
                include: "history",
                message: e.to_string(),
            }),
        }
    }

    if !warnings.is_empty() {
        obj.insert(
            "warnings".to_string(),
            serde_json::to_value(warnings).unwrap_or(Value::Null),
        );
    }

    InspectOutcome::Success(obj)
}

/// A link counts as a subtask/parent relationship if the user's
/// `[backend.link_mappings]` config maps the canonical `subtask` or `parent`
/// keyword to the link's type name, or if the type or direction descriptions
/// mention subtask, parent, or child (covers YouTrack "Subtask" / "is parent
/// for", Jira "Subtask", Linear "Subtask" / "is subtask of"). Renamed or
/// localized hierarchy link types without a mapping entry are not recognized.
fn is_subtask_link(link: &IssueLink, link_mappings: &HashMap<String, String>) -> bool {
    let lt = &link.link_type;
    if ["subtask", "parent"].iter().any(|key| {
        link_mappings
            .get(*key)
            .is_some_and(|mapped| unicode_eq_ignore_case(mapped, &lt.name))
    }) {
        return true;
    }
    let mut hay = lt.name.to_lowercase();
    for part in [&lt.source_to_target, &lt.target_to_source]
        .into_iter()
        .flatten()
    {
        hay.push(' ');
        hay.push_str(&part.to_lowercase());
    }
    hay.contains("subtask") || hay.contains("parent") || hay.contains("child")
}

/// Parse `--include` values (already split on commas by clap) into flags.
fn parse_includes(raw: &[String]) -> Result<Includes> {
    let mut inc = Includes::default();
    for item in raw {
        match item.trim().to_ascii_lowercase().as_str() {
            "" => {}
            "comments" => inc.comments = true,
            "links" => inc.links = true,
            "subtasks" => inc.subtasks = true,
            "history" => inc.history = true,
            "all" => {
                inc = Includes {
                    comments: true,
                    links: true,
                    subtasks: true,
                    history: true,
                };
            }
            other => {
                return Err(anyhow!(
                    "Unknown --include value '{}'. Valid values: comments, links, subtasks, history, all",
                    other
                ));
            }
        }
    }
    Ok(inc)
}

/// Collect issue IDs from positional args and/or an ID file ("-" for stdin),
/// deduplicated while preserving input order.
fn collect_ids(positional: &[String], ids_file: Option<&Path>) -> Result<Vec<String>> {
    let mut ids: Vec<String> = positional
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if let Some(path) = ids_file {
        let content = if path.as_os_str() == "-" {
            let mut buf = String::new();
            std::io::stdin()
                .lock()
                .read_to_string(&mut buf)
                .context("Failed to read issue IDs from stdin")?;
            buf
        } else {
            std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read issue IDs from '{}'", path.display()))?
        };
        ids.extend(parse_id_lines(&content));
    }

    Ok(dedup_preserve_order(ids))
}

/// Parse ID-file content: one ID per line, trimmed; blank lines and
/// `#`-comment lines are skipped.
fn parse_id_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect()
}

fn dedup_preserve_order(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// Emit the stable top-level JSON report object. `total` counts the results
/// in this report; `query_total` is the backend-reported match count (query
/// mode only, omitted when unknown).
fn output_report_json(
    outcomes: &[InspectOutcome],
    total: usize,
    succeeded: usize,
    failed: usize,
    query_total: Option<u64>,
) -> Result<()> {
    let mut issues: Vec<&Map<String, Value>> = Vec::new();
    let mut errors: Vec<&InspectError> = Vec::new();
    for outcome in outcomes {
        match outcome {
            InspectOutcome::Success(obj) => issues.push(obj),
            InspectOutcome::Failure(err) => errors.push(err),
        }
    }
    let mut report = serde_json::json!({
        "total": total,
        "succeeded": succeeded,
        "failed": failed,
        "issues": issues,
        "errors": errors,
    });
    if let Some(matched) = query_total
        && let Some(obj) = report.as_object_mut()
    {
        obj.insert("query_total".to_string(), matched.into());
    }
    output_json(&report)
}

/// Emit one compact JSON object for a single issue result, flushed
/// immediately so `--jsonl` streams results as they are processed.
fn output_jsonl_line(outcome: &InspectOutcome) -> Result<()> {
    let line = match outcome {
        InspectOutcome::Success(obj) => serde_json::to_string(&JsonlSuccess {
            success: true,
            issue: obj,
        })?,
        InspectOutcome::Failure(err) => serde_json::to_string(&serde_json::json!({
            "success": false,
            "id": err.id,
            "error": err.error,
        }))?,
    };
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{}", line)?;
    stdout.flush()?;
    Ok(())
}

fn output_report_text(
    outcomes: &[InspectOutcome],
    total: usize,
    succeeded: usize,
    failed: usize,
    query_total: Option<u64>,
) {
    use colored::Colorize;

    println!(
        "Inspected {} issue{}: {} succeeded, {} failed",
        total,
        if total == 1 { "" } else { "s" },
        succeeded,
        failed
    );
    if let Some(matched) = query_total
        && matched > total as u64
    {
        println!(
            "Query matched {} issues; this report covers {} (use --limit/--skip or --all to fetch more)",
            matched, total
        );
    }

    for outcome in outcomes {
        let InspectOutcome::Success(obj) = outcome else {
            continue;
        };
        let id = obj
            .get("id_readable")
            .and_then(Value::as_str)
            .or_else(|| obj.get("id").and_then(Value::as_str))
            .unwrap_or("?");
        let summary = obj.get("summary").and_then(Value::as_str).unwrap_or("");
        println!("\n{} - {}", id.cyan().bold(), summary);

        for key in ["comments", "links", "subtasks", "history"] {
            if let Some(Value::Array(items)) = obj.get(key) {
                println!("  {}: {}", key, items.len());
            }
        }
        if let Some(Value::Array(warnings)) = obj.get("warnings") {
            for warning in warnings {
                let include = warning
                    .get("include")
                    .and_then(Value::as_str)
                    .unwrap_or("?");
                let message = warning.get("message").and_then(Value::as_str).unwrap_or("");
                println!("  {} {}: {}", "warning".yellow(), include, message);
            }
        }
    }

    let errors: Vec<&InspectError> = outcomes
        .iter()
        .filter_map(|o| match o {
            InspectOutcome::Failure(err) => Some(err),
            InspectOutcome::Success(_) => None,
        })
        .collect();
    if !errors.is_empty() {
        println!("\n{}:", "Errors".red().bold());
        for err in errors {
            println!("  {}: {}", err.id.cyan(), err.error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_includes_individual_values() {
        let inc = parse_includes(&["comments".to_string(), "links".to_string()]).unwrap();
        assert!(inc.comments);
        assert!(inc.links);
        assert!(!inc.subtasks);
        assert!(!inc.history);
    }

    #[test]
    fn parse_includes_all_expands_everything() {
        let inc = parse_includes(&["all".to_string()]).unwrap();
        assert!(inc.comments && inc.links && inc.subtasks && inc.history);
    }

    #[test]
    fn parse_includes_is_case_insensitive_and_trims() {
        let inc = parse_includes(&[" History ".to_string(), "SUBTASKS".to_string()]).unwrap();
        assert!(inc.history);
        assert!(inc.subtasks);
        assert!(!inc.comments);
    }

    #[test]
    fn parse_includes_rejects_unknown_value() {
        let err = parse_includes(&["attachments".to_string()]).unwrap_err();
        assert!(err.to_string().contains("attachments"));
        assert!(
            err.to_string()
                .contains("comments, links, subtasks, history, all")
        );
    }

    #[test]
    fn parse_includes_empty_is_default_fast() {
        let inc = parse_includes(&[]).unwrap();
        assert_eq!(inc, Includes::default());
    }

    #[test]
    fn parse_id_lines_skips_blanks_and_comments() {
        let ids = parse_id_lines("PROJ-1\n\n  PROJ-2  \n# a comment\nPROJ-3\n");
        assert_eq!(ids, vec!["PROJ-1", "PROJ-2", "PROJ-3"]);
    }

    #[test]
    fn dedup_preserves_first_occurrence_order() {
        let ids = dedup_preserve_order(vec![
            "B-2".to_string(),
            "A-1".to_string(),
            "B-2".to_string(),
            "C-3".to_string(),
            "A-1".to_string(),
        ]);
        assert_eq!(ids, vec!["B-2", "A-1", "C-3"]);
    }

    #[test]
    fn collect_ids_merges_positional_and_file() {
        let dir = std::env::temp_dir().join("track-test-inspect-ids");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ids.txt");
        std::fs::write(&file, "PROJ-2\nPROJ-3\n# skip\nPROJ-1\n").unwrap();

        let ids = collect_ids(&["PROJ-1".to_string()], Some(&file)).unwrap();
        assert_eq!(ids, vec!["PROJ-1", "PROJ-2", "PROJ-3"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn collect_ids_errors_on_missing_file() {
        let missing = Path::new("/nonexistent/track-inspect-ids.txt");
        let err = collect_ids(&[], Some(missing)).unwrap_err();
        assert!(err.to_string().contains("Failed to read issue IDs"));
    }

    fn mk_link(name: &str, s2t: Option<&str>, t2s: Option<&str>) -> IssueLink {
        use tracker_core::IssueLinkType;
        IssueLink {
            id: "l".to_string(),
            direction: None,
            link_type: IssueLinkType {
                id: "t".to_string(),
                name: name.to_string(),
                source_to_target: s2t.map(String::from),
                target_to_source: t2s.map(String::from),
                directed: true,
            },
            issues: vec![],
        }
    }

    #[test]
    fn is_subtask_link_matches_by_type_name_and_directions() {
        let no_mappings = HashMap::new();

        assert!(is_subtask_link(
            &mk_link("Subtask", None, None),
            &no_mappings
        ));
        assert!(is_subtask_link(
            &mk_link("Hierarchy", Some("is parent for"), Some("is subtask of")),
            &no_mappings
        ));
        assert!(!is_subtask_link(
            &mk_link("Relates", Some("relates to"), Some("relates to")),
            &no_mappings
        ));
        assert!(!is_subtask_link(
            &mk_link("Depend", Some("is required for"), Some("depends on")),
            &no_mappings
        ));
    }

    #[test]
    fn is_subtask_link_honors_custom_link_mappings() {
        let link = mk_link("Blocks Chain", Some("blocks"), Some("is blocked by"));

        // Without a mapping the custom name is not recognized as hierarchy
        assert!(!is_subtask_link(&link, &HashMap::new()));

        // Canonical subtask/parent keys mapped to the custom name (any case) match
        let subtask_map = HashMap::from([("subtask".to_string(), "blocks chain".to_string())]);
        assert!(is_subtask_link(&link, &subtask_map));
        let parent_map = HashMap::from([("parent".to_string(), "Blocks Chain".to_string())]);
        assert!(is_subtask_link(&link, &parent_map));

        // Other canonical keys mapped to this name do not classify it
        let depends_map = HashMap::from([("depends".to_string(), "Blocks Chain".to_string())]);
        assert!(!is_subtask_link(&link, &depends_map));
    }

    #[test]
    fn outcome_counts_aggregate_success_and_failure() {
        let outcomes = [
            InspectOutcome::Success(Map::new()),
            InspectOutcome::Failure(InspectError {
                id: "X-1".to_string(),
                error: "boom".to_string(),
            }),
            InspectOutcome::Success(Map::new()),
        ];
        let (succeeded, failed) = outcomes.iter().fold((0usize, 0usize), |(s, f), o| match o {
            InspectOutcome::Success(_) => (s + 1, f),
            InspectOutcome::Failure(_) => (s, f + 1),
        });
        assert_eq!((succeeded, failed), (2, 1));
        assert_eq!(outcomes.len(), 3);
    }
}
