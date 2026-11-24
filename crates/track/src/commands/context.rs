use crate::cache::{
    CachedBackendMetadata, CachedIssueCount, CachedLinkType, CachedProject, CachedQueryTemplate,
    CachedRecentIssue, CachedTag, CachedUser, ProjectFieldsCache, ProjectUsersCache,
    ProjectWorkflowHints, STALE_AFTER, TrackerCache,
};
use crate::cli::{Backend, OutputFormat};
use crate::config::{LoadedWorkflowPack, pack_query_named, require_valid_workflow_pack};
use crate::output::{AGENT_ISSUE_HEADER, agent_issue_row, output_json};
use anyhow::{Context, Result};
use colored::Colorize;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use tracker_core::{Issue, IssueTracker, KnowledgeBase};

/// Aggregated context for AI assistants - single JSON blob with all relevant data
#[derive(Serialize)]
pub struct AggregatedContext {
    /// Timestamp when this context was generated
    pub generated_at: String,
    /// When the cached data below was last fetched from the tracker
    pub cache_updated_at: Option<String>,
    /// Age of the cached data in seconds
    pub cache_age_seconds: Option<i64>,
    /// True when the cached data is more than a day old
    pub stale: bool,
    /// The user the credentials authenticate as, when the backend reports it
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_user: Option<CachedUser>,
    /// Backend metadata (type and URL)
    pub backend: Option<CachedBackendMetadata>,
    /// Default project from config
    pub default_project: Option<String>,
    /// List of projects with their IDs
    pub projects: Vec<CachedProject>,
    /// Custom fields per project (with enum values for dropdowns)
    pub project_fields: Vec<ProjectFieldsCache>,
    /// Available tags
    pub tags: Vec<CachedTag>,
    /// Available issue link types
    pub link_types: Vec<CachedLinkType>,
    /// Selected workflow pack (unexpanded queries). Not persisted in cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_pack: Option<Value>,
    /// Pre-built query templates for the backend
    pub query_templates: Vec<CachedQueryTemplate>,
    /// Assignable users per project
    pub assignable_users: Vec<ProjectUsersCache>,
    /// Workflow hints: the states of each state field, in workflow order
    pub workflow_hints: Vec<ProjectWorkflowHints>,
    /// Recently accessed issues (from cache LRU)
    pub recent_issues: Vec<CachedRecentIssue>,
    /// Issue counts per project and template (from cached counts)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issue_counts: Vec<CachedIssueCount>,
    /// Unresolved issues (if --include-issues was specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issues: Option<Vec<IssueSummary>>,
}

/// Lightweight issue summary for context output
#[derive(Serialize)]
pub struct IssueSummary {
    pub id: String,
    pub id_readable: String,
    pub summary: String,
    pub project: String,
    pub state: Option<String>,
    pub priority: Option<String>,
    pub assignee: Option<String>,
}

impl From<&Issue> for IssueSummary {
    fn from(issue: &Issue) -> Self {
        let fields = issue.common_fields();
        let state = fields.state;
        let priority = fields.priority;
        let assignee = fields.assignee;

        IssueSummary {
            id: issue.id.clone(),
            id_readable: issue.id_readable.clone(),
            summary: issue.summary.clone(),
            project: issue
                .project
                .short_name
                .clone()
                .unwrap_or_else(|| issue.project.id.clone()),
            state: state.cloned(),
            priority: priority.cloned(),
            assignee: assignee.cloned(),
        }
    }
}

/// Handle the context command
#[allow(clippy::too_many_arguments)]
pub fn handle_context(
    client: &dyn IssueTracker,
    _kb_client: Option<&dyn KnowledgeBase>,
    project: Option<&str>,
    refresh: bool,
    include_issues: bool,
    issue_limit: usize,
    format: OutputFormat,
    backend: Backend,
    url: Option<&str>,
    default_project: Option<&str>,
    explicit_config: Option<&Path>,
) -> Result<()> {
    let backend_type = backend.to_string();
    let refresh_from_api = || -> Result<TrackerCache> {
        let cache = TrackerCache::refresh(
            client,
            &backend_type,
            url.unwrap_or("unknown"),
            default_project,
        )
        .context("Failed to refresh cache from API")?;
        cache.save(None)?;
        cache.remember_default_project(client);
        Ok(cache)
    };

    let mut cache = if refresh {
        refresh_from_api()?
    } else {
        let mut loaded = TrackerCache::load(None).unwrap_or_default();
        let _ = loaded.ensure_projects();
        // An empty cache, or one written for another instance (the URL
        // changed, or a committed .tracker-cache/), is fetched again rather
        // than presented as this tracker's data.
        if loaded.projects.is_empty() || !loaded.matches_active() {
            refresh_from_api()?
        } else {
            loaded
        }
    };

    let loaded_pack = require_valid_workflow_pack(explicit_config)?;
    let issues_project = project.or(default_project);

    // For an agent the briefing is the context: same cache, a fraction of
    // the bytes, and nothing cut without a pointer to the rest. It reads
    // only what it prints, so the aggregate below is never built.
    if format == OutputFormat::Text && crate::output::is_agent() {
        print!(
            "{}",
            super::prime::briefing(
                &mut cache,
                &super::prime::PrimeOptions {
                    project,
                    max_values: super::prime::DEFAULT_MAX_VALUES,
                    format,
                    backend,
                    url,
                    default_project,
                    explicit_config,
                    hook: None,
                },
            )
        );
        if include_issues
            && let Some(issues) =
                unresolved_issues(client, &backend_type, issues_project, issue_limit)
        {
            println!("unresolved issues ({}):", issues.len());
            println!("{AGENT_ISSUE_HEADER}");
            for issue in &issues {
                println!("{}", agent_issue_row(issue));
            }
        }
        return Ok(());
    }

    // Ensure full data for context output
    cache.ensure_all_loaded()?;

    // Templates fitted to the project in view, when its schema is cached.
    let query_templates = super::workflow::overlay_query_templates(
        loaded_pack.as_ref().map(|loaded| &loaded.pack),
        cache.templates_for(issues_project),
        &backend_type,
    );
    let mut issue_counts = cache.issue_counts.clone();
    if let Some(loaded) = &loaded_pack {
        issue_counts.retain(|count| pack_query_named(&loaded.pack, &count.template_name).is_none());
    }

    // Build aggregated context
    let cache_age = cache.age();
    let mut context = AggregatedContext {
        generated_at: chrono::Utc::now().to_rfc3339(),
        cache_updated_at: cache.updated_at.clone(),
        cache_age_seconds: cache_age.map(|age| age.num_seconds()),
        stale: cache.is_stale(STALE_AFTER),
        current_user: cache.current_user.clone(),
        backend: cache.backend_metadata.clone(),
        default_project: default_project.map(|s| s.to_string()),
        projects: cache.projects.clone(),
        project_fields: cache.project_fields.clone(),
        tags: cache.tags.clone(),
        link_types: cache.link_types.clone(),
        workflow_pack: loaded_pack.as_ref().map(super::workflow::context_pack_json),
        query_templates,
        assignable_users: cache.project_users.clone(),
        workflow_hints: cache.workflow_hints.clone(),
        recent_issues: cache.recent_issues.clone(),
        issue_counts,
        issues: None,
    };

    // Filter to specific project if requested
    if let Some(proj) = project {
        context
            .projects
            .retain(|p| p.short_name.eq_ignore_ascii_case(proj) || p.id == proj);
        context
            .project_fields
            .retain(|pf| pf.project_short_name.eq_ignore_ascii_case(proj) || pf.project_id == proj);
        context
            .assignable_users
            .retain(|pu| pu.project_short_name.eq_ignore_ascii_case(proj) || pu.project_id == proj);
        context
            .workflow_hints
            .retain(|wh| wh.project_short_name.eq_ignore_ascii_case(proj) || wh.project_id == proj);
        context
            .recent_issues
            .retain(|ri| ri.project_short_name.eq_ignore_ascii_case(proj));
        context
            .issue_counts
            .retain(|ic| ic.project_short_name.eq_ignore_ascii_case(proj));
    }

    // Fetch unresolved issues if requested
    if include_issues {
        context.issues = unresolved_issues(client, &backend_type, issues_project, issue_limit)
            .map(|issues| issues.iter().map(IssueSummary::from).collect());
    }

    // Output
    match format {
        OutputFormat::Json => {
            output_json(&context)?;
        }
        OutputFormat::Text => {
            println!("{}:", "AI Context".white().bold());
            println!("  {}: {}", "Generated".dimmed(), context.generated_at);
            if context.stale {
                println!(
                    "  {}: {} {}",
                    "Cache".dimmed(),
                    cache.age_string().yellow(),
                    "(stale: run 'track cache refresh' or 'track context --refresh')".yellow()
                );
            } else {
                println!("  {}: {}", "Cache".dimmed(), cache.age_string());
            }
            if let Some(user) = &context.current_user {
                println!(
                    "  {}: {}",
                    "You".dimmed(),
                    user.login.as_deref().unwrap_or(&user.display_name).cyan()
                );
            }

            if let Some(meta) = &context.backend {
                println!(
                    "  {}: {} ({})",
                    "Backend".dimmed(),
                    meta.backend_type.cyan(),
                    meta.base_url.dimmed()
                );
            }

            if let Some(proj) = &context.default_project {
                println!("  {}: {}", "Default project".dimmed(), proj.cyan().bold());
            }

            if let Some(loaded) = &loaded_pack {
                print_workflow_pack_text(loaded);
            }

            println!();
            println!("{}:", "Projects".white().bold());
            for p in &context.projects {
                println!("  {} - {}", p.short_name.cyan().bold(), p.name);
            }

            if !context.issue_counts.is_empty() {
                println!();
                println!("{}:", "Issue Counts".white().bold());
                // One line per project, in first-seen order
                let mut projects_seen = std::collections::HashSet::new();
                for ic in &context.issue_counts {
                    if projects_seen.insert(&ic.project_short_name) {
                        let counts: Vec<String> = context
                            .issue_counts
                            .iter()
                            .filter(|c| c.project_short_name == ic.project_short_name)
                            .map(|c| format!("{}: {}", c.template_name, c.count.to_string().cyan()))
                            .collect();
                        println!(
                            "  {}: {}",
                            ic.project_short_name.cyan(),
                            counts.join("  \u{00b7}  ")
                        );
                    }
                }
            }

            if !context.project_fields.is_empty() {
                println!();
                println!("{}:", "Custom Fields".white().bold());
                for pf in &context.project_fields {
                    println!("  {}:", pf.project_short_name.cyan());
                    for f in &pf.fields {
                        let required_str = if f.required {
                            " *required".red().bold().to_string()
                        } else {
                            String::new()
                        };
                        let values_str = if f.values.is_empty() {
                            String::new()
                        } else if f.values.len() <= 5 {
                            format!(" [{}]", f.values.join(", "))
                        } else {
                            format!(
                                " [{}, ... ({} values)]",
                                f.values[..3].join(", "),
                                f.values.len()
                            )
                        };
                        println!(
                            "    {} ({}){}{}",
                            f.name.white(),
                            f.field_type.dimmed(),
                            required_str,
                            values_str.dimmed()
                        );
                    }
                    // Summary line for required fields
                    let required_names: Vec<&str> = pf
                        .fields
                        .iter()
                        .filter(|f| f.required)
                        .map(|f| f.name.as_str())
                        .collect();
                    if !required_names.is_empty() {
                        println!(
                            "    {}: {}",
                            "Required".red().bold(),
                            required_names.join(", ").white()
                        );
                    }
                }
            }

            if !context.query_templates.is_empty() {
                println!();
                println!("{}:", "Query Templates".white().bold());
                for qt in &context.query_templates {
                    let pack_mark = loaded_pack
                        .as_ref()
                        .is_some_and(|loaded| pack_query_named(&loaded.pack, &qt.name).is_some());
                    if pack_mark {
                        println!(
                            "  {}: {} {}",
                            qt.name.cyan(),
                            qt.description.dimmed(),
                            "[pack, higher precedence]".yellow()
                        );
                    } else {
                        println!("  {}: {}", qt.name.cyan(), qt.description.dimmed());
                    }
                    println!("    {}", qt.query.dimmed());
                }
            }

            if !context.link_types.is_empty() {
                println!();
                println!("{}:", "Link Types".white().bold());
                for lt in &context.link_types {
                    println!("  {}", lt.name.cyan());
                }
            }

            if !context.assignable_users.is_empty() {
                println!();
                println!("{}:", "Assignable Users".white().bold());
                for pu in &context.assignable_users {
                    let count = pu.users.len();
                    let sample: Vec<&str> = pu
                        .users
                        .iter()
                        .take(5)
                        .map(|u| u.display_name.as_str())
                        .collect();
                    let sample_str = if count > 5 {
                        format!("{}, ... ({} total)", sample.join(", "), count)
                    } else {
                        sample.join(", ")
                    };
                    println!(
                        "  {}: {}",
                        pu.project_short_name.cyan(),
                        sample_str.dimmed()
                    );
                }
            }

            if !context.workflow_hints.is_empty() {
                println!();
                println!("{}:", "Workflow Hints".white().bold());
                for wh in &context.workflow_hints {
                    for sf in &wh.state_fields {
                        println!(
                            "  {} ({}):",
                            wh.project_short_name.cyan(),
                            sf.field_name.white()
                        );

                        // Show states in order with resolved marker
                        let states_str: Vec<String> = sf
                            .states
                            .iter()
                            .map(|s| {
                                if s.is_resolved {
                                    format!("{}*", s.name)
                                } else {
                                    s.name.clone()
                                }
                            })
                            .collect();
                        println!(
                            "    States: {} (* = resolved)",
                            states_str.join(" → ").dimmed()
                        );
                    }
                }
            }

            if !context.recent_issues.is_empty() {
                println!();
                println!("{}:", "Recent Issues".white().bold());
                for ri in context.recent_issues.iter().take(10) {
                    let state = ri.state.as_deref().unwrap_or("?");
                    println!(
                        "  {} [{}] {}",
                        ri.id_readable.cyan(),
                        state.dimmed(),
                        ri.summary
                    );
                }
            }

            if let Some(issues) = &context.issues {
                println!();
                println!("{}:", "Unresolved Issues".white().bold());
                for issue in issues {
                    let state = issue.state.as_deref().unwrap_or("?");
                    let priority = issue.priority.as_deref().unwrap_or("");
                    let assignee = issue
                        .assignee
                        .as_deref()
                        .map(|a| format!(" @{}", a))
                        .unwrap_or_default();
                    println!(
                        "  {} [{}] {}{} - {}",
                        issue.id_readable.cyan(),
                        state.dimmed(),
                        priority.yellow(),
                        assignee.dimmed(),
                        issue.summary
                    );
                }
            }

            println!();
            println!("{}", "Use -o json for machine-readable output.".dimmed());
        }
    }

    Ok(())
}

/// The unresolved issues `--include-issues` asks for. A failure to fetch them
/// is a warning on stderr, not a failed command.
fn unresolved_issues(
    client: &dyn IssueTracker,
    backend_type: &str,
    project: Option<&str>,
    limit: usize,
) -> Option<Vec<Issue>> {
    let Some(query) = unresolved_issues_query(backend_type, project) else {
        eprintln!(
            "Warning: --include-issues needs --project or a default project; skipping issues"
        );
        return None;
    };
    match client.search_issues(&query, limit, 0) {
        Ok(result) => Some(result.items),
        Err(e) => {
            eprintln!("Warning: Failed to fetch issues: {}", e);
            None
        }
    }
}

/// Search query for unresolved issues in the backend's query syntax.
///
/// GitHub and GitLab clients are already scoped to one repository/project, so
/// they need no project (`track init` sets no default project for them); the
/// other backends return `None` without one.
fn unresolved_issues_query(backend_type: &str, project: Option<&str>) -> Option<String> {
    match (backend_type, project) {
        ("gitlab", _) => Some("state=opened".to_string()),
        ("github", Some(project)) => Some(format!("repo:{} is:issue state:open", project)),
        ("github", None) => Some("is:issue state:open".to_string()),
        ("jira", Some(project)) => Some(format!("project = {} AND resolution IS EMPTY", project)),
        // YouTrack and Linear
        (_, Some(project)) => Some(format!("project: {} #Unresolved", project)),
        (_, None) => None,
    }
}

fn print_workflow_pack_text(loaded: &LoadedWorkflowPack) {
    println!();
    println!("{}:", "Workflow Pack".white().bold());
    println!(
        "  {}: {} ({})",
        "Name".dimmed(),
        loaded.pack.name.cyan().bold(),
        loaded.source.as_str()
    );
    if let Some(description) = &loaded.pack.description {
        println!("  {}: {}", "Description".dimmed(), description);
    }
    if let Some(project) = &loaded.pack.default_project {
        println!("  {}: {}", "Default project".dimmed(), project.cyan());
    }
    println!(
        "  {}",
        "Pack queries take precedence over cached built-ins.".yellow()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_issues_query_needs_project_only_for_unscoped_backends() {
        assert_eq!(
            unresolved_issues_query("gitlab", None).as_deref(),
            Some("state=opened")
        );
        assert_eq!(
            unresolved_issues_query("github", None).as_deref(),
            Some("is:issue state:open")
        );
        assert_eq!(unresolved_issues_query("youtrack", None), None);
        assert_eq!(unresolved_issues_query("jira", None), None);
        assert_eq!(
            unresolved_issues_query("jira", Some("PROJ")).as_deref(),
            Some("project = PROJ AND resolution IS EMPTY")
        );
        assert_eq!(
            unresolved_issues_query("linear", Some("ENG")).as_deref(),
            Some("project: ENG #Unresolved")
        );
    }
}
