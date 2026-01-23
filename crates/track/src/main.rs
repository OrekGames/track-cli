mod cache;
mod cli;
mod color;
mod commands;
mod config;
mod local_config;
mod output;

use anyhow::Result;
use clap::Parser;
use cli::{Backend, Cli, Commands};
use config::Config;
use output::output_error;
use std::process::ExitCode;
use tracker_core::{IssueTracker, KnowledgeBase};
use youtrack_backend::YouTrackClient;

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Initialize color mode based on CLI flag and environment
    color::init(cli.color);

    if let Err(e) = run(cli) {
        output_error(&e, cli::OutputFormat::Text);
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

fn run(cli: Cli) -> Result<()> {
    // Handle config commands that don't need API connection
    if let Commands::Config { action } = &cli.command {
        use cli::ConfigCommands;
        match action {
            ConfigCommands::Show | ConfigCommands::Clear | ConfigCommands::Path => {
                return handle_config_local(action, cli.format);
            }
            ConfigCommands::Project { .. } => {
                // Project command needs API to resolve project name
            }
        }
    }

    let mut config = Config::load(cli.config.clone(), cli.backend)?;
    config.merge_with_cli(cli.url.clone(), cli.token.clone());
    config.validate(cli.backend)?;

    // Create the appropriate backend client
    // We use the concrete client type to support both IssueTracker and KnowledgeBase
    match cli.backend {
        Backend::YouTrack => {
            let client = YouTrackClient::new(
                config.url.as_ref().unwrap(),
                config.token.as_ref().unwrap(),
            );
            run_with_client(&client, &client, &cli)
        }
    }
}

/// Run commands with clients that implement the required traits
fn run_with_client(
    issue_client: &dyn IssueTracker,
    kb_client: &dyn KnowledgeBase,
    cli: &Cli,
) -> Result<()> {
    match &cli.command {
        Commands::Issue { action } => {
            commands::issue::handle_issue(issue_client, action, cli.format)
        }
        Commands::Project { action } => {
            commands::project::handle_project(issue_client, action, cli.format)
        }
        Commands::Tags { action } => {
            commands::tags::handle_tags(issue_client, action, cli.format)
        }
        Commands::Cache { action } => {
            handle_cache(issue_client, action, cli.format, cli.backend)
        }
        Commands::Config { action } => handle_config(issue_client, action, cli.format),
        Commands::Article { action } => {
            commands::article::handle_article(issue_client, kb_client, action, cli.format)
        }
    }
}

fn handle_cache(
    client: &dyn IssueTracker,
    action: &cli::CacheCommands,
    format: cli::OutputFormat,
    backend: Backend,
) -> Result<()> {
    use cli::CacheCommands;

    match action {
        CacheCommands::Refresh => {
            // For now, cache only works with YouTrack backend
            match backend {
                Backend::YouTrack => {
                    // We need the concrete client for cache refresh
                    // This is a temporary solution until cache is made backend-agnostic
                    let cache = cache::TrackerCache::refresh(client)?;
                    cache.save(None)?;
                    match format {
                        cli::OutputFormat::Json => {
                            println!(r#"{{"success": true, "message": "Cache refreshed"}}"#);
                        }
                        cli::OutputFormat::Text => {
                            use colored::Colorize;
                            println!("{}", "Cache refreshed successfully".green());
                            println!("  {}: {}", "Projects".dimmed(), cache.projects.len());
                            println!("  {}: {}", "Tags".dimmed(), cache.tags.len());
                        }
                    }
                }
            }
            Ok(())
        }
        CacheCommands::Show => {
            let cache = cache::TrackerCache::load(None)?;
            match format {
                cli::OutputFormat::Json => {
                    let json = serde_json::to_string_pretty(&cache)?;
                    println!("{}", json);
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
                            println!(
                                "    {} [{}]{}",
                                f.name.white(),
                                f.field_type.dimmed(),
                                req
                            );
                        }
                    }
                    println!();
                    println!("{}:", "Tags".white().bold());
                    for t in &cache.tags {
                        println!("  {} ({})", t.name.magenta(), t.id.dimmed());
                    }
                }
            }
            Ok(())
        }
        CacheCommands::Path => {
            let path = std::env::current_dir()?.join(".tracker-cache.json");
            println!("{}", path.display());
            Ok(())
        }
    }
}

/// Handle config commands that don't need API connection
fn handle_config_local(action: &cli::ConfigCommands, format: cli::OutputFormat) -> Result<()> {
    use cli::ConfigCommands;
    use local_config::LocalConfig;

    match action {
        ConfigCommands::Show => {
            let local_config = LocalConfig::load()?;
            match format {
                cli::OutputFormat::Json => {
                    let json = serde_json::to_string_pretty(&local_config)?;
                    println!("{}", json);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    if local_config.is_empty() {
                        println!("No local configuration set.");
                        println!(
                            "Use '{}' to set a default project.",
                            "track config project <ID>".cyan()
                        );
                    } else {
                        println!("{}:", "Local configuration".white().bold());
                        if let (Some(id), Some(name)) = (
                            &local_config.default_project_id,
                            &local_config.default_project_name,
                        ) {
                            println!(
                                "  {}: {} ({})",
                                "Default project".dimmed(),
                                name.cyan().bold(),
                                id.dimmed()
                            );
                        }
                    }
                }
            }
            Ok(())
        }
        ConfigCommands::Clear => {
            LocalConfig::delete()?;
            match format {
                cli::OutputFormat::Json => {
                    println!(r#"{{"success": true, "message": "Local configuration cleared"}}"#);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{}", "Local configuration cleared.".green());
                }
            }
            Ok(())
        }
        ConfigCommands::Path => {
            let path = LocalConfig::config_path()?;
            println!("{}", path.display());
            Ok(())
        }
        ConfigCommands::Project { .. } => {
            // This shouldn't be called - requires API
            unreachable!("Project command should be handled by handle_config")
        }
    }
}

fn handle_config(
    client: &dyn IssueTracker,
    action: &cli::ConfigCommands,
    format: cli::OutputFormat,
) -> Result<()> {
    use cli::ConfigCommands;
    use local_config::LocalConfig;

    match action {
        ConfigCommands::Project { id } => {
            // Resolve project to get both ID and shortName
            let projects = client.list_projects()?;
            let project = projects
                .iter()
                .find(|p| {
                    p.short_name.eq_ignore_ascii_case(id)
                        || p.id == *id
                        || p.name.eq_ignore_ascii_case(id)
                })
                .ok_or_else(|| anyhow::anyhow!("Project '{}' not found", id))?;

            let mut local_config = LocalConfig::load()?;
            local_config.set_default_project(project.id.clone(), project.short_name.clone());
            local_config.save()?;

            match format {
                cli::OutputFormat::Json => {
                    println!(
                        r#"{{"success": true, "default_project_id": "{}", "default_project_name": "{}"}}"#,
                        project.id, project.short_name
                    );
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!(
                        "Default project set to: {} ({})",
                        project.short_name.cyan().bold(),
                        project.id.dimmed()
                    );
                }
            }
            Ok(())
        }
        // These are handled by handle_config_local before API validation
        ConfigCommands::Show | ConfigCommands::Clear | ConfigCommands::Path => {
            unreachable!("Local config commands should be handled before API validation")
        }
    }
}
