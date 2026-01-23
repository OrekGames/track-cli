mod cache;
mod cli;
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
                            println!("Cache refreshed successfully");
                            println!("  Projects: {}", cache.projects.len());
                            println!("  Tags: {}", cache.tags.len());
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
                    if let Some(updated) = &cache.updated_at {
                        println!("Last updated: {}", updated);
                    } else {
                        println!("Cache is empty. Run 'track cache refresh' to populate.");
                        return Ok(());
                    }
                    println!();
                    println!("Projects:");
                    for p in &cache.projects {
                        println!("  {} ({}) - {}", p.short_name, p.id, p.name);
                    }
                    println!();
                    println!("Custom Fields by Project:");
                    for pf in &cache.project_fields {
                        println!("  {}:", pf.project_short_name);
                        for f in &pf.fields {
                            let req = if f.required { " (required)" } else { "" };
                            println!("    {} [{}]{}", f.name, f.field_type, req);
                        }
                    }
                    println!();
                    println!("Tags:");
                    for t in &cache.tags {
                        println!("  {} ({})", t.name, t.id);
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
                    if local_config.is_empty() {
                        println!("No local configuration set.");
                        println!("Use 'track config project <ID>' to set a default project.");
                    } else {
                        println!("Local configuration:");
                        if let (Some(id), Some(name)) = (
                            &local_config.default_project_id,
                            &local_config.default_project_name,
                        ) {
                            println!("  Default project: {} ({})", name, id);
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
                    println!("Local configuration cleared.");
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
                    println!(
                        "Default project set to: {} ({})",
                        project.short_name, project.id
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
