use crate::cache::{
    CachedBackendMetadata, CachedLinkType, CachedProject, CachedQueryTemplate, CachedRecentIssue,
    CachedTag, ProjectFieldsCache, ProjectUsersCache, ProjectWorkflowHints, TrackerCache,
};
use crate::cli::OutputFormat;
use anyhow::{Context, Result};
use colored::Colorize;
use serde::Serialize;
use tracker_core::{Issue, IssueTracker, KnowledgeBase};

/// Aggregated context for AI assistants - single JSON blob with all relevant data
#[derive(Serialize)]
pub struct AggregatedContext {
    /// Timestamp when this context was generated
    pub generated_at: String,
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
    /// Pre-built query templates for the backend
    pub query_templates: Vec<CachedQueryTemplate>,
    /// Assignable users per project
    pub assignable_users: Vec<ProjectUsersCache>,
    /// Workflow hints: valid state transitions per project
    pub workflow_hints: Vec<ProjectWorkflowHints>,
    /// Recently accessed issues (from cache LRU)
    pub recent_issues: Vec<CachedRecentIssue>,
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
        // Extract state from custom fields
        let state = issue.custom_fields.iter().find_map(|cf| {
            if let tracker_core::CustomField::State { value, .. } = cf {
                value.clone()
            } else {
                None
            }
        });

        // Extract priority from custom fields
        let priority = issue.custom_fields.iter().find_map(|cf| {
            if let tracker_core::CustomField::SingleEnum { name, value } = cf {
                if name.to_lowercase() == "priority" {
                    value.clone()
                } else {
                    None
                }
            } else {
                None
            }
        });

        // Extract assignee from custom fields
        let assignee = issue.custom_fields.iter().find_map(|cf| {
            if let tracker_core::CustomField::SingleUser { login, .. } = cf {
                login.clone()
            } else {
                None
            }
        });

        IssueSummary {
            id: issue.id.clone(),
            id_readable: issue.id_readable.clone(),
            summary: issue.summary.clone(),
            project: issue
                .project
                .short_name
                .clone()
                .unwrap_or_else(|| issue.project.id.clone()),
            state,
            priority,
            assignee,
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
    backend_type: &str,
    base_url: &str,
    default_project: Option<&str>,
) -> Result<()> {
    // Load or refresh cache
    let cache = if refresh {
        // Force refresh from API
        let cache = TrackerCache::refresh(client, backend_type, base_url, default_project)
            .context("Failed to refresh cache from API")?;
        cache.save(None)?;
        cache
    } else {
        // Try to load existing cache, refresh if empty
        let loaded = TrackerCache::load(None).unwrap_or_default();
        if loaded.projects.is_empty() {
            let cache = TrackerCache::refresh(client, backend_type, base_url, default_project)
                .context("Failed to refresh cache from API")?;
            cache.save(None)?;
            cache
        } else {
            loaded
        }
    };

    // Build aggregated context
    let mut context = AggregatedContext {
        generated_at: chrono::Utc::now().to_rfc3339(),
        backend: cache.backend_metadata.clone(),
        default_project: default_project.map(|s| s.to_string()),
        projects: cache.projects.clone(),
        project_fields: cache.project_fields.clone(),
        tags: cache.tags.clone(),
        link_types: cache.link_types.clone(),
        query_templates: cache.query_templates.clone(),
        assignable_users: cache.project_users.clone(),
        workflow_hints: cache.workflow_hints.clone(),
        recent_issues: cache.recent_issues.clone(),
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
    }

    // Fetch unresolved issues if requested
    if include_issues {
        let target_project = project.or(default_project);
        if let Some(proj) = target_project {
            // Build query based on backend type
            let query = match backend_type {
                "jira" => format!("project = {} AND resolution IS EMPTY", proj),
                _ => format!("project: {} #Unresolved", proj), // YouTrack default
            };

            match client.search_issues(&query, issue_limit, 0) {
                Ok(issues) => {
                    context.issues = Some(issues.iter().map(IssueSummary::from).collect());
                }
                Err(e) => {
                    // Don't fail the whole command, just skip issues
                    eprintln!("Warning: Failed to fetch issues: {}", e);
                }
            }
        }
    }

    // Output
    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&context)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            println!("{}:", "AI Context".white().bold());
            println!("  {}: {}", "Generated".dimmed(), context.generated_at);

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

            println!();
            println!("{}:", "Projects".white().bold());
            for p in &context.projects {
                println!("  {} - {}", p.short_name.cyan().bold(), p.name);
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
                    println!("  {}: {}", qt.name.cyan(), qt.description.dimmed());
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
                        println!("    States: {}", states_str.join(" → ").dimmed());

                        // Show forward transitions summary
                        let forward_count = sf
                            .transitions
                            .iter()
                            .filter(|t| t.transition_type == "forward")
                            .count();
                        let backward_count = sf
                            .transitions
                            .iter()
                            .filter(|t| t.transition_type == "backward")
                            .count();
                        println!(
                            "    Transitions: {} forward, {} backward (* = resolved)",
                            forward_count.to_string().green(),
                            backward_count.to_string().yellow()
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
