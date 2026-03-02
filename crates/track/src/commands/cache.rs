use anyhow::Result;

use crate::cache;
use crate::cli::{self, Backend};
use crate::config::Config;
use crate::output::output_json;
use tracker_core::{IssueTracker, KnowledgeBase};

pub fn handle_cache(
    client: &dyn IssueTracker,
    kb_client: Option<&dyn KnowledgeBase>,
    action: &cli::CacheCommands,
    format: cli::OutputFormat,
    backend: Backend,
    config: &Config,
) -> Result<()> {
    use cli::CacheCommands;

    // Log cache commands for eval scoring when in mock mode
    if let Some(mock_dir) = tracker_mock::get_mock_dir() {
        let method = match action {
            CacheCommands::Refresh { .. } => "cache_refresh",
            CacheCommands::Status => "cache_status",
            CacheCommands::Show => "cache_show",
            CacheCommands::Path => "cache_path",
        };
        tracker_mock::log_cli_command(&mock_dir, method, &[]);
    }

    match action {
        CacheCommands::Refresh { if_stale } => {
            // Check if we should skip refresh based on --if-stale
            if let Some(stale_duration) = if_stale {
                let max_age = cache::parse_duration(stale_duration)?;
                let existing_cache = cache::TrackerCache::load(None).unwrap_or_default();

                if !existing_cache.is_stale(max_age) {
                    // Cache is fresh, skip refresh
                    match format {
                        cli::OutputFormat::Json => {
                            let age_seconds =
                                existing_cache.age().map(|a| a.num_seconds()).unwrap_or(0);
                            output_json(&serde_json::json!({
                                "success": true,
                                "skipped": true,
                                "message": "Cache is fresh",
                                "age_seconds": age_seconds
                            }))?;
                        }
                        cli::OutputFormat::Text => {
                            use colored::Colorize;
                            println!(
                                "{} (last updated {})",
                                "Cache is fresh, skipping refresh".green(),
                                existing_cache.age_string().cyan()
                            );
                        }
                    }
                    return Ok(());
                }
            }

            // Cache works with any backend that implements IssueTracker
            let backend_type = backend.to_string();
            let base_url = config.url.as_deref().unwrap_or("unknown");
            let default_project = config.default_project.as_deref();

            let cache = cache::TrackerCache::refresh_with_articles(
                client,
                kb_client,
                &backend_type,
                base_url,
                default_project,
            )?;
            cache.save(None)?;

            match format {
                cli::OutputFormat::Json => {
                    output_json(&serde_json::json!({
                        "success": true,
                        "message": "Cache refreshed"
                    }))?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{}", "Cache refreshed successfully".green());
                    println!("  {}: {}", "Backend".dimmed(), backend_type.cyan());
                    println!("  {}: {}", "Projects".dimmed(), cache.projects.len());
                    println!("  {}: {}", "Tags".dimmed(), cache.tags.len());
                    println!("  {}: {}", "Link types".dimmed(), cache.link_types.len());
                    println!(
                        "  {}: {}",
                        "Query templates".dimmed(),
                        cache.query_templates.len()
                    );
                    if !cache.project_users.is_empty() {
                        let total_users: usize =
                            cache.project_users.iter().map(|p| p.users.len()).sum();
                        println!("  {}: {}", "Users".dimmed(), total_users);
                    }
                    if !cache.articles.is_empty() {
                        println!("  {}: {}", "Articles".dimmed(), cache.articles.len());
                    }
                }
            }
            Ok(())
        }
        CacheCommands::Status => {
            let mut cache = cache::TrackerCache::load(None).unwrap_or_default();
            // Load only what we need for status
            let _ = cache.ensure_backend_shards();
            let _ = cache.ensure_projects();
            let _ = cache.ensure_runtime_shards();

            match format {
                cli::OutputFormat::Json => {
                    let age_seconds = cache.age().map(|a| a.num_seconds());
                    let updated_at = cache.updated_at.clone();
                    let is_empty = cache.is_empty();

                    let status = serde_json::json!({
                        "exists": !is_empty,
                        "updated_at": updated_at,
                        "age_seconds": age_seconds,
                        "age_human": cache.age_string(),
                        "projects_count": cache.projects.len(),
                        "tags_count": cache.tags.len(),
                        "recent_issues_count": cache.recent_issues.len(),
                    });
                    output_json(&status)?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    if cache.is_empty() {
                        println!("{}: {}", "Cache status".white().bold(), "empty".yellow());
                        println!(
                            "  Run '{}' to populate the cache.",
                            "track cache refresh".cyan()
                        );
                        return Ok(());
                    }

                    let age_str = cache.age_string();
                    let freshness = if let Some(age) = cache.age() {
                        if age.num_hours() < 1 {
                            "fresh".green().to_string()
                        } else if age.num_hours() < 24 {
                            "recent".yellow().to_string()
                        } else {
                            "stale".red().to_string()
                        }
                    } else {
                        "unknown".dimmed().to_string()
                    };

                    println!("{}: {}", "Cache status".white().bold(), freshness);
                    println!("  {}: {}", "Last updated".dimmed(), age_str.cyan());
                    if let Some(updated) = &cache.updated_at {
                        println!("  {}: {}", "Timestamp".dimmed(), updated.dimmed());
                    }
                    if let Some(meta) = &cache.backend_metadata {
                        println!("  {}: {}", "Backend".dimmed(), meta.backend_type.cyan());
                    }
                    println!("  {}: {}", "Projects".dimmed(), cache.projects.len());
                    println!("  {}: {}", "Tags".dimmed(), cache.tags.len());
                    println!(
                        "  {}: {}",
                        "Recent issues".dimmed(),
                        cache.recent_issues.len()
                    );

                    // Suggest refresh if stale
                    if let Some(age) = cache.age()
                        && age.num_hours() >= 24 {
                            println!();
                            println!(
                                "  {} Run '{}' to update.",
                                "Tip:".yellow(),
                                "track cache refresh".cyan()
                            );
                        }
                }
            }
            Ok(())
        }
        CacheCommands::Show => {
            let mut cache = cache::TrackerCache::load(None)?;
            cache.ensure_all_loaded()?;
            match format {
                cli::OutputFormat::Json => {
                    output_json(&cache)?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    if let Some(updated) = &cache.updated_at {
                        println!("{}: {}", "Last updated".dimmed(), updated);
                    } else {
                        println!(
                            "Cache is empty. Run '{}' to populate.",
                            "track cache refresh".cyan()
                        );
                        return Ok(());
                    }

                    // Backend metadata
                    if let Some(meta) = &cache.backend_metadata {
                        println!(
                            "{}: {} ({})",
                            "Backend".dimmed(),
                            meta.backend_type.cyan(),
                            meta.base_url.dimmed()
                        );
                    }
                    if let Some(proj) = &cache.default_project {
                        println!("{}: {}", "Default project".dimmed(), proj.cyan().bold());
                    }

                    println!();
                    println!("{}:", "Projects".white().bold());
                    for p in &cache.projects {
                        println!(
                            "  {} ({}) - {}",
                            p.short_name.cyan().bold(),
                            p.id.dimmed(),
                            p.name
                        );
                    }

                    println!();
                    println!("{}:", "Custom Fields by Project".white().bold());
                    for pf in &cache.project_fields {
                        println!("  {}:", pf.project_short_name.cyan());
                        for f in &pf.fields {
                            let req = if f.required {
                                " (required)".yellow().to_string()
                            } else {
                                String::new()
                            };
                            let values_str = if f.values.is_empty() {
                                String::new()
                            } else {
                                format!(" -> {}", f.values.join(", ").dimmed())
                            };
                            println!(
                                "    {} [{}]{}{}",
                                f.name.white(),
                                f.field_type.dimmed(),
                                req,
                                values_str
                            );
                        }
                    }

                    println!();
                    println!("{}:", "Tags/Labels".white().bold());
                    if cache.tags.is_empty() {
                        println!("  {}", "(none)".dimmed());
                    } else {
                        for t in &cache.tags {
                            let color_str = t
                                .color
                                .as_deref()
                                .map(|c| format!(" [{}]", c))
                                .unwrap_or_default();
                            println!(
                                "  {} ({}){}",
                                t.name.magenta(),
                                t.id.dimmed(),
                                color_str.dimmed()
                            );
                        }
                    }

                    // Link types
                    if !cache.link_types.is_empty() {
                        println!();
                        println!("{}:", "Link Types".white().bold());
                        for lt in &cache.link_types {
                            let outward = lt.source_to_target.as_deref().unwrap_or("-");
                            let inward = lt.target_to_source.as_deref().unwrap_or("-");
                            println!(
                                "  {} ({} / {})",
                                lt.name.cyan(),
                                outward.dimmed(),
                                inward.dimmed()
                            );
                        }
                    }

                    // Query templates
                    if !cache.query_templates.is_empty() {
                        println!();
                        println!("{}:", "Query Templates".white().bold());
                        for qt in &cache.query_templates {
                            println!("  {}: {}", qt.name.cyan(), qt.description.dimmed());
                        }
                    }

                    // Project users
                    if !cache.project_users.is_empty() {
                        println!();
                        println!("{}:", "Project Users".white().bold());
                        for pu in &cache.project_users {
                            let user_count = pu.users.len();
                            let sample_users: Vec<&str> = pu
                                .users
                                .iter()
                                .take(3)
                                .map(|u| u.display_name.as_str())
                                .collect();
                            let sample_str = if user_count > 3 {
                                format!("{}, ... ({} total)", sample_users.join(", "), user_count)
                            } else {
                                sample_users.join(", ")
                            };
                            println!(
                                "  {}: {}",
                                pu.project_short_name.cyan(),
                                sample_str.dimmed()
                            );
                        }
                    }

                    // Recent issues
                    if !cache.recent_issues.is_empty() {
                        println!();
                        println!("{}:", "Recent Issues".white().bold());
                        for ri in cache.recent_issues.iter().take(10) {
                            let state = ri.state.as_deref().unwrap_or("?");
                            println!(
                                "  {} [{}] {}",
                                ri.id_readable.cyan(),
                                state.dimmed(),
                                ri.summary
                            );
                        }
                    }

                    // Articles
                    if !cache.articles.is_empty() {
                        println!();
                        println!("{}:", "Articles".white().bold());
                        for a in cache.articles.iter().take(10) {
                            let children = if a.has_children { " (+)" } else { "" };
                            println!(
                                "  {}{} - {}",
                                a.id_readable.cyan(),
                                children.dimmed(),
                                a.summary
                            );
                        }
                        if cache.articles.len() > 10 {
                            println!("  {} more...", cache.articles.len() - 10);
                        }
                    }
                }
            }
            Ok(())
        }
        CacheCommands::Path => {
            let path = std::env::current_dir()?.join(".tracker-cache");
            println!("{}", path.display());
            Ok(())
        }
    }
}
