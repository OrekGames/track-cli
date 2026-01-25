mod cache;
mod cli;
mod color;
mod commands;
mod config;
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
    // Handle completions command - no API needed
    if let Commands::Completions { shell } = &cli.command {
        Cli::generate_completions(*shell);
        return Ok(());
    }

    // Handle init command - creates config, no existing auth needed
    if let Commands::Init {
        url,
        token,
        project,
    } = &cli.command
    {
        return handle_init(url, token, project.as_deref(), cli.format, cli.backend);
    }

    // Handle config commands that don't need API connection
    if let Commands::Config { action } = &cli.command {
        use cli::ConfigCommands;
        match action {
            ConfigCommands::Show | ConfigCommands::Clear | ConfigCommands::Path => {
                return handle_config_local(action, cli.format);
            }
            ConfigCommands::Project { .. } | ConfigCommands::Test => {
                // These commands need API connection
            }
        }
    }

    // Handle external commands (shortcuts) early if they are clearly invalid
    // to provide better error messages when config is missing
    if let Commands::External(args) = &cli.command {
        if let Some(cmd) = args.first() {
            if !is_issue_id(cmd) {
                return Err(anyhow::anyhow!(
                    "unrecognized subcommand '{}'. Run 'track --help' for usage.",
                    cmd
                ));
            }
        } else {
            return Err(anyhow::anyhow!(
                "unrecognized subcommand. Run 'track --help' for usage."
            ));
        }
    }

    let mut config = Config::load(cli.config.clone(), cli.backend)?;
    config.merge_with_cli(cli.url.clone(), cli.token.clone());
    config.validate(cli.backend)?;

    // Create the appropriate backend client
    // We use the concrete client type to support both IssueTracker and KnowledgeBase
    match cli.backend {
        Backend::YouTrack => {
            let client =
                YouTrackClient::new(config.url.as_ref().unwrap(), config.token.as_ref().unwrap());
            run_with_client(&client, &client, &cli, &config)
        }
    }
}

/// Run commands with clients that implement the required traits
fn run_with_client(
    issue_client: &dyn IssueTracker,
    kb_client: &dyn KnowledgeBase,
    cli: &Cli,
    config: &Config,
) -> Result<()> {
    match &cli.command {
        Commands::Issue { action } => {
            commands::issue::handle_issue(issue_client, action, cli.format, config.default_project.as_deref())
        }
        Commands::Project { action } => {
            commands::project::handle_project(issue_client, action, cli.format)
        }
        Commands::Tags { action } => commands::tags::handle_tags(issue_client, action, cli.format),
        Commands::Cache { action } => handle_cache(issue_client, action, cli.format, cli.backend),
        Commands::Config { action } => handle_config(issue_client, action, cli.format, config),
        Commands::Article { action } => {
            commands::article::handle_article(issue_client, kb_client, action, cli.format)
        }
        Commands::Completions { .. } => {
            // Already handled before config loading
            unreachable!("Completions command should be handled before API validation")
        }
        Commands::Init { .. } => {
            // Already handled before config loading
            unreachable!("Init command should be handled before API validation")
        }
        Commands::Open { id } => {
            handle_open(id.as_deref(), config, cli.format)
        }
        Commands::External(args) => {
            // Handle shortcut: `track PROJ-123` as `track issue get PROJ-123`
            handle_issue_shortcut(issue_client, args, cli.format)
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
                            println!("    {} [{}]{}", f.name.white(), f.field_type.dimmed(), req);
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

    match action {
        ConfigCommands::Show => {
            let config = Config::load_local_track_toml()?;
            match format {
                cli::OutputFormat::Json => {
                    if let Some(cfg) = &config {
                        let output = serde_json::json!({
                            "default_project": cfg.default_project,
                            "url": cfg.url,
                            "has_token": cfg.token.is_some()
                        });
                        println!("{}", serde_json::to_string_pretty(&output)?);
                    } else {
                        println!("{{}}");
                    }
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    if let Some(cfg) = config {
                        let config_path = config::local_track_config_path()?;
                        println!("{}:", "Configuration".white().bold());
                        println!("  {}: {}", "File".dimmed(), config_path.display());
                        if let Some(url) = &cfg.url {
                            println!("  {}: {}", "URL".dimmed(), url.cyan());
                        }
                        if cfg.token.is_some() {
                            println!("  {}: {}", "Token".dimmed(), "(set)".green());
                        }
                        if let Some(project) = &cfg.default_project {
                            println!(
                                "  {}: {}",
                                "Default project".dimmed(),
                                project.cyan().bold()
                            );
                        }
                    } else {
                        println!("No .track.toml configuration found.");
                        println!(
                            "Run '{}' to create one.",
                            "track init --url <URL> --token <TOKEN>".cyan()
                        );
                    }
                }
            }
            Ok(())
        }
        ConfigCommands::Clear => {
            // Clear default_project from .track.toml (keep url/token)
            let config_path = config::local_track_config_path()?;
            if let Some(mut cfg) = Config::load_local_track_toml()? {
                cfg.default_project = None;
                cfg.save(&config_path)?;
                match format {
                    cli::OutputFormat::Json => {
                        println!(r#"{{"success": true, "message": "Default project cleared"}}"#);
                    }
                    cli::OutputFormat::Text => {
                        use colored::Colorize;
                        println!("{}", "Default project cleared.".green());
                    }
                }
            } else {
                match format {
                    cli::OutputFormat::Json => {
                        println!(r#"{{"success": true, "message": "No configuration to clear"}}"#);
                    }
                    cli::OutputFormat::Text => {
                        println!("No .track.toml configuration found.");
                    }
                }
            }
            Ok(())
        }
        ConfigCommands::Path => {
            let path = config::local_track_config_path()?;
            println!("{}", path.display());
            Ok(())
        }
        ConfigCommands::Project { .. } | ConfigCommands::Test => {
            // This shouldn't be called - requires API
            unreachable!("Project/Test command should be handled by handle_config")
        }
    }
}

fn handle_config(
    client: &dyn IssueTracker,
    action: &cli::ConfigCommands,
    format: cli::OutputFormat,
    config: &Config,
) -> Result<()> {
    use cli::ConfigCommands;

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

            // Update .track.toml with the default project
            Config::update_default_project(&project.short_name)?;

            match format {
                cli::OutputFormat::Json => {
                    println!(
                        r#"{{"success": true, "default_project": "{}"}}"#,
                        project.short_name
                    );
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!(
                        "Default project set to: {}",
                        project.short_name.cyan().bold()
                    );
                }
            }
            Ok(())
        }
        ConfigCommands::Test => {
            // Test connection by fetching current user info via projects list
            let projects = client.list_projects()?;
            let url = config.url.as_deref().unwrap_or("unknown");

            match format {
                cli::OutputFormat::Json => {
                    println!(
                        r#"{{"success": true, "url": "{}", "projects_count": {}}}"#,
                        url,
                        projects.len()
                    );
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!(
                        "{} Connected to {}",
                        "✓".green().bold(),
                        url.cyan()
                    );
                    println!(
                        "  {} projects accessible",
                        projects.len().to_string().white().bold()
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

fn handle_init(
    url: &str,
    token: &str,
    project: Option<&str>,
    format: cli::OutputFormat,
    backend: Backend,
) -> Result<()> {
    use colored::Colorize;

    // Validate URL format
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow::anyhow!(
            "Invalid URL: must start with http:// or https://"
        ));
    }

    let config_path = config::local_track_config_path()?;

    // Check if config already exists
    if config_path.exists() {
        return Err(anyhow::anyhow!(
            "Config file already exists: {}\nUse a text editor to modify it, or delete it first.",
            config_path.display()
        ));
    }

    // If project is specified, validate it against the server
    let validated_project = if let Some(proj) = project {
        // Create temporary client to validate project
        let client = match backend {
            Backend::YouTrack => YouTrackClient::new(url, token),
        };

        let projects = client.list_projects().map_err(|e| {
            anyhow::anyhow!(
                "Failed to connect to server or list projects: {}\nCheck your URL and token.",
                e
            )
        })?;

        let matched = projects
            .iter()
            .find(|p| {
                p.short_name.eq_ignore_ascii_case(proj)
                    || p.id == proj
                    || p.name.eq_ignore_ascii_case(proj)
            })
            .ok_or_else(|| anyhow::anyhow!("Project '{}' not found on server", proj))?;

        Some((matched.id.clone(), matched.short_name.clone()))
    } else {
        None
    };

    // Create config with optional default project
    let config = Config {
        url: Some(url.to_string()),
        token: Some(token.to_string()),
        default_project: validated_project.as_ref().map(|(_, name)| name.clone()),
        youtrack: Default::default(),
    };

    config.save(&config_path)?;

    match format {
        cli::OutputFormat::Json => {
            let project_json = if let Some((_, name)) = &validated_project {
                format!(r#", "default_project": "{}""#, name)
            } else {
                String::new()
            };
            println!(
                r#"{{"success": true, "config_path": "{}"{}}}"#,
                config_path.display(),
                project_json
            );
        }
        cli::OutputFormat::Text => {
            println!(
                "{} {}",
                "Created config file:".green(),
                config_path.display()
            );
            if let Some((_, name)) = &validated_project {
                println!("  {}: {}", "Default project".dimmed(), name.cyan().bold());
            }
            println!();
            println!(
                "{}",
                "You can now use track commands without --url and --token flags.".dimmed()
            );
        }
    }

    Ok(())
}

fn handle_open(
    id: Option<&str>,
    config: &Config,
    format: cli::OutputFormat,
) -> Result<()> {
    use colored::Colorize;

    let base_url = config
        .url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No URL configured"))?;

    // Remove trailing slash from base URL if present
    let base_url = base_url.trim_end_matches('/');

    let url = if let Some(issue_id) = id {
        // Open specific issue: https://youtrack.example.com/issue/PROJ-123
        format!("{}/issue/{}", base_url, issue_id)
    } else {
        // Open dashboard
        base_url.to_string()
    };

    // Try to open in browser
    let result = open::that(&url);

    match format {
        cli::OutputFormat::Json => {
            if result.is_ok() {
                println!(r#"{{"success": true, "url": "{}"}}"#, url);
            } else {
                println!(r#"{{"success": false, "url": "{}", "error": "Failed to open browser"}}"#, url);
            }
        }
        cli::OutputFormat::Text => {
            if result.is_ok() {
                if let Some(issue_id) = id {
                    println!("Opening {} in browser...", issue_id.cyan().bold());
                } else {
                    println!("Opening dashboard in browser...");
                }
            } else {
                // If we can't open the browser, at least print the URL
                println!("Could not open browser. URL: {}", url.cyan());
            }
        }
    }

    Ok(())
}

/// Check if a string looks like an issue ID (e.g., PROJ-123)
fn is_issue_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 2
        && !parts[0].is_empty()
        && parts[0].chars().all(|c| c.is_ascii_alphanumeric())
        && !parts[1].is_empty()
        && parts[1].chars().all(|c| c.is_ascii_digit())
}

fn handle_issue_shortcut(
    client: &dyn IssueTracker,
    args: &[String],
    format: cli::OutputFormat,
) -> Result<()> {
    // Check if the first argument looks like an issue ID (e.g., PROJ-123)
    if args.is_empty() {
        return Err(anyhow::anyhow!(
            "unrecognized subcommand. Run 'track --help' for usage."
        ));
    }

    let potential_id = &args[0];

    if !is_issue_id(potential_id) {
        return Err(anyhow::anyhow!(
            "unrecognized subcommand '{}'. Run 'track --help' for usage.",
            potential_id
        ));
    }

    // Check for --full flag in remaining args
    let full = args.iter().any(|a| a == "--full");

    // Treat as `track issue get <ID>`
    let issue = client
        .get_issue(potential_id)
        .map_err(|e| anyhow::anyhow!("Failed to fetch issue '{}': {}", potential_id, e))?;

    if !full {
        output::output_result(&issue, format);
        return Ok(());
    }

    // Fetch additional context for full view
    let links = client.get_issue_links(potential_id)?;
    let comments = client.get_comments(potential_id)?;

    match format {
        cli::OutputFormat::Json => {
            let full_issue = serde_json::json!({
                "issue": issue,
                "links": links,
                "comments": comments
            });
            println!("{}", serde_json::to_string_pretty(&full_issue)?);
        }
        cli::OutputFormat::Text => {
            use colored::Colorize;
            output::output_result(&issue, format);

            if !links.is_empty() {
                println!("\n  {}:", "Links".dimmed());
                for link in &links {
                    let direction = link.direction.as_deref().unwrap_or("BOTH");
                    let description = match direction {
                        "INWARD" => link
                            .link_type
                            .target_to_source
                            .as_deref()
                            .unwrap_or(&link.link_type.name),
                        "OUTWARD" => link
                            .link_type
                            .source_to_target
                            .as_deref()
                            .unwrap_or(&link.link_type.name),
                        _ => &link.link_type.name,
                    };
                    for linked_issue in &link.issues {
                        let linked_id = linked_issue
                            .id_readable
                            .as_deref()
                            .unwrap_or(&linked_issue.id);
                        let linked_summary = linked_issue.summary.as_deref().unwrap_or("");
                        println!(
                            "    {} {} - {}",
                            description.dimmed(),
                            linked_id.cyan(),
                            linked_summary
                        );
                    }
                }
            }

            if !comments.is_empty() {
                let recent_comments: Vec<_> = comments.iter().rev().take(5).collect();
                println!(
                    "\n  {} ({} total):",
                    "Recent Comments".dimmed(),
                    comments.len()
                );
                for comment in recent_comments.iter().rev() {
                    let author = comment
                        .author
                        .as_ref()
                        .map(|a| a.login.as_str())
                        .unwrap_or("unknown");
                    let date = comment
                        .created
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default();
                    println!("\n    [{}] {} wrote:", date.dimmed(), author.cyan());
                    for line in comment.text.lines().take(3) {
                        println!("      {}", line);
                    }
                    if comment.text.lines().count() > 3 {
                        println!("      ...");
                    }
                }
            }
        }
    }

    Ok(())
}
