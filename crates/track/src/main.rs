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
use jira_backend::{ConfluenceClient, JiraClient};
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
        backend,
        email,
    } = &cli.command
    {
        return handle_init(
            url,
            token,
            project.as_deref(),
            email.as_deref(),
            cli.format,
            *backend,
        );
    }

    // Handle config commands that don't need API connection
    if let Commands::Config { action } = &cli.command {
        use cli::ConfigCommands;
        match action {
            ConfigCommands::Show
            | ConfigCommands::Clear
            | ConfigCommands::Path
            | ConfigCommands::Keys
            | ConfigCommands::Set { .. }
            | ConfigCommands::Get { .. } => {
                return handle_config_local(action, cli.format);
            }
            ConfigCommands::Backend { backend } => {
                return handle_config_backend(*backend, cli.format);
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

    // Determine effective backend: CLI flag takes precedence, then config, then default
    let effective_backend = cli.backend.unwrap_or_else(|| {
        // Try to get backend from config file
        Config::load_local_track_toml()
            .ok()
            .flatten()
            .map(|c| c.get_backend())
            .unwrap_or(Backend::YouTrack)
    });

    let mut config = Config::load(cli.config.clone(), effective_backend)?;
    config.merge_with_cli(cli.url.clone(), cli.token.clone());
    config.validate(effective_backend)?;

    // Create the appropriate backend client
    // We use the concrete client type to support both IssueTracker and KnowledgeBase
    match effective_backend {
        Backend::YouTrack => {
            let client =
                YouTrackClient::new(config.url.as_ref().unwrap(), config.token.as_ref().unwrap());
            run_with_client(&client, &client, &cli, &config)
        }
        Backend::Jira => {
            let url = config.url.as_ref().unwrap();
            let email = config.email.as_ref().unwrap();
            let token = config.token.as_ref().unwrap();

            let client = JiraClient::new(url, email, token);
            // Confluence is Atlassian's knowledge base, at the same domain with /wiki path
            let confluence = ConfluenceClient::new(url, email, token);
            run_with_client(&client, &confluence, &cli, &config)
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
        Commands::Issue { action } => commands::issue::handle_issue(
            issue_client,
            action,
            cli.format,
            config.default_project.as_deref(),
        ),
        Commands::Project { action } => {
            commands::project::handle_project(issue_client, action, cli.format)
        }
        Commands::Tags { action } => commands::tags::handle_tags(issue_client, action, cli.format),
        Commands::Cache { action } => {
            let backend = cli.backend.unwrap_or_else(|| config.get_backend());
            handle_cache(issue_client, Some(kb_client), action, cli.format, backend, config)
        }
        Commands::Config { action } => handle_config(issue_client, action, cli.format, config),
        Commands::Article { action } => {
            commands::article::handle_article(issue_client, kb_client, action, cli.format)
        }
        Commands::Context {
            project,
            refresh,
            include_issues,
            issue_limit,
        } => {
            let backend = cli.backend.unwrap_or_else(|| config.get_backend());
            let backend_type = match backend {
                Backend::YouTrack => "youtrack",
                Backend::Jira => "jira",
            };
            commands::context::handle_context(
                issue_client,
                Some(kb_client),
                project.as_deref(),
                *refresh,
                *include_issues,
                *issue_limit,
                cli.format,
                backend_type,
                config.url.as_deref().unwrap_or("unknown"),
                config.default_project.as_deref(),
            )
        }
        Commands::Completions { .. } => {
            // Already handled before config loading
            unreachable!("Completions command should be handled before API validation")
        }
        Commands::Init { .. } => {
            // Already handled before config loading
            unreachable!("Init command should be handled before API validation")
        }
        Commands::Open { id } => handle_open(id.as_deref(), config, cli.format),
        Commands::External(args) => {
            // Handle shortcut: `track PROJ-123` as `track issue get PROJ-123`
            handle_issue_shortcut(issue_client, args, cli.format)
        }
    }
}

fn handle_cache(
    client: &dyn IssueTracker,
    kb_client: Option<&dyn KnowledgeBase>,
    action: &cli::CacheCommands,
    format: cli::OutputFormat,
    backend: Backend,
    config: &Config,
) -> Result<()> {
    use cli::CacheCommands;

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
                            let age_seconds = existing_cache.age().map(|a| a.num_seconds()).unwrap_or(0);
                            println!(
                                r#"{{"success": true, "skipped": true, "message": "Cache is fresh", "age_seconds": {}}}"#,
                                age_seconds
                            );
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
            let backend_type = match backend {
                Backend::YouTrack => "youtrack",
                Backend::Jira => "jira",
            };
            let base_url = config.url.as_deref().unwrap_or("unknown");
            let default_project = config.default_project.as_deref();

            let cache = cache::TrackerCache::refresh_with_articles(client, kb_client, backend_type, base_url, default_project)?;
            cache.save(None)?;

            match format {
                cli::OutputFormat::Json => {
                    println!(r#"{{"success": true, "message": "Cache refreshed"}}"#);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{}", "Cache refreshed successfully".green());
                    println!("  {}: {}", "Backend".dimmed(), backend_type.cyan());
                    println!("  {}: {}", "Projects".dimmed(), cache.projects.len());
                    println!("  {}: {}", "Tags".dimmed(), cache.tags.len());
                    println!("  {}: {}", "Link types".dimmed(), cache.link_types.len());
                    println!("  {}: {}", "Query templates".dimmed(), cache.query_templates.len());
                    if !cache.project_users.is_empty() {
                        let total_users: usize = cache.project_users.iter().map(|p| p.users.len()).sum();
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
            let cache = cache::TrackerCache::load(None).unwrap_or_default();

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
                    println!("{}", serde_json::to_string_pretty(&status)?);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    if cache.is_empty() {
                        println!(
                            "{}: {}",
                            "Cache status".white().bold(),
                            "empty".yellow()
                        );
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
                    println!("  {}: {}", "Recent issues".dimmed(), cache.recent_issues.len());

                    // Suggest refresh if stale
                    if let Some(age) = cache.age() {
                        if age.num_hours() >= 24 {
                            println!();
                            println!(
                                "  {} Run '{}' to update.",
                                "Tip:".yellow(),
                                "track cache refresh".cyan()
                            );
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

                    // Backend metadata
                    if let Some(meta) = &cache.backend_metadata {
                        println!("{}: {} ({})", "Backend".dimmed(), meta.backend_type.cyan(), meta.base_url.dimmed());
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
                            println!("    {} [{}]{}{}", f.name.white(), f.field_type.dimmed(), req, values_str);
                        }
                    }

                    println!();
                    println!("{}:", "Tags".white().bold());
                    if cache.tags.is_empty() {
                        println!("  {}", "(none)".dimmed());
                    } else {
                        for t in &cache.tags {
                            println!("  {} ({})", t.name.magenta(), t.id.dimmed());
                        }
                    }

                    // Link types
                    if !cache.link_types.is_empty() {
                        println!();
                        println!("{}:", "Link Types".white().bold());
                        for lt in &cache.link_types {
                            let outward = lt.source_to_target.as_deref().unwrap_or("-");
                            let inward = lt.target_to_source.as_deref().unwrap_or("-");
                            println!("  {} ({} / {})", lt.name.cyan(), outward.dimmed(), inward.dimmed());
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
                            let sample_users: Vec<&str> = pu.users.iter().take(3).map(|u| u.display_name.as_str()).collect();
                            let sample_str = if user_count > 3 {
                                format!("{}, ... ({} total)", sample_users.join(", "), user_count)
                            } else {
                                sample_users.join(", ")
                            };
                            println!("  {}: {}", pu.project_short_name.cyan(), sample_str.dimmed());
                        }
                    }

                    // Recent issues
                    if !cache.recent_issues.is_empty() {
                        println!();
                        println!("{}:", "Recent Issues".white().bold());
                        for ri in cache.recent_issues.iter().take(10) {
                            let state = ri.state.as_deref().unwrap_or("?");
                            println!("  {} [{}] {}", ri.id_readable.cyan(), state.dimmed(), ri.summary);
                        }
                    }

                    // Articles
                    if !cache.articles.is_empty() {
                        println!();
                        println!("{}:", "Articles".white().bold());
                        for a in cache.articles.iter().take(10) {
                            let children = if a.has_children { " (+)" } else { "" };
                            println!("  {}{} - {}", a.id_readable.cyan(), children.dimmed(), a.summary);
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
            let path = std::env::current_dir()?.join(".tracker-cache.json");
            println!("{}", path.display());
            Ok(())
        }
    }
}

/// Handle config backend command
fn handle_config_backend(backend: Backend, format: cli::OutputFormat) -> Result<()> {
    Config::update_backend(backend)?;
    let backend_name = match backend {
        Backend::YouTrack => "youtrack",
        Backend::Jira => "jira",
    };

    match format {
        cli::OutputFormat::Json => {
            println!(r#"{{"success": true, "backend": "{}"}}"#, backend_name);
        }
        cli::OutputFormat::Text => {
            use colored::Colorize;
            println!("Default backend set to: {}", backend_name.cyan().bold());
        }
    }
    Ok(())
}

/// All valid configuration keys
const VALID_CONFIG_KEYS: &[(&str, &str, &str)] = &[
    ("backend", "youtrack | jira", "Default backend to use"),
    ("url", "string", "Tracker instance URL"),
    ("token", "string", "API token (YouTrack permanent token or Jira API token)"),
    ("email", "string", "Email for authentication (required for Jira)"),
    ("default_project", "string", "Default project shortName (e.g., \"PROJ\")"),
    ("youtrack.url", "string", "YouTrack-specific URL (overrides 'url' when backend=youtrack)"),
    ("youtrack.token", "string", "YouTrack-specific token"),
    ("jira.url", "string", "Jira-specific URL (overrides 'url' when backend=jira)"),
    ("jira.email", "string", "Jira-specific email"),
    ("jira.token", "string", "Jira-specific token"),
];

/// Handle config commands that don't need API connection
fn handle_config_local(action: &cli::ConfigCommands, format: cli::OutputFormat) -> Result<()> {
    use cli::ConfigCommands;

    match action {
        ConfigCommands::Keys => {
            match format {
                cli::OutputFormat::Json => {
                    let keys: Vec<serde_json::Value> = VALID_CONFIG_KEYS
                        .iter()
                        .map(|(key, value_type, description)| {
                            serde_json::json!({
                                "key": key,
                                "type": value_type,
                                "description": description
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&keys)?);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{}:", "Available Configuration Keys".white().bold());
                    println!();
                    for (key, value_type, description) in VALID_CONFIG_KEYS {
                        println!("  {} ({})", key.cyan().bold(), value_type.dimmed());
                        println!("    {}", description);
                    }
                    println!();
                    println!("{}:", "Example .track.toml file".white().bold());
                    println!(r#"
  # Global settings
  backend = "youtrack"
  default_project = "PROJ"

  # YouTrack-specific settings
  [youtrack]
  url = "https://youtrack.example.com"
  token = "perm:xxx"

  # Jira-specific settings (used when backend = "jira")
  [jira]
  url = "https://company.atlassian.net"
  email = "user@company.com"
  token = "api-token"
"#);
                    println!("{}:", "Usage".white().bold());
                    println!("  Set a value:  {}", "track config set <key> <value>".cyan());
                    println!("  Get a value:  {}", "track config get <key>".cyan());
                    println!("  Show config:  {}", "track config show".cyan());
                }
            }
            Ok(())
        }
        ConfigCommands::Set { key, value } => {
            // Validate key
            let valid_key = VALID_CONFIG_KEYS.iter().any(|(k, _, _)| *k == key);
            if !valid_key {
                return Err(anyhow::anyhow!(
                    "Invalid configuration key: '{}'\nRun 'track config keys' to see valid keys.",
                    key
                ));
            }

            let config_path = config::local_track_config_path()?;
            let mut cfg = Config::load_local_track_toml()?.unwrap_or_default();

            // Set the value based on the key
            match key.as_str() {
                "backend" => {
                    if value != "youtrack" && value != "jira" && value != "yt" && value != "j" {
                        return Err(anyhow::anyhow!(
                            "Invalid backend value: '{}'. Use 'youtrack' or 'jira'.",
                            value
                        ));
                    }
                    let normalized = if value == "yt" { "youtrack" } else if value == "j" { "jira" } else { value };
                    cfg.backend = Some(normalized.to_string());
                }
                "url" => cfg.url = Some(value.clone()),
                "token" => cfg.token = Some(value.clone()),
                "email" => cfg.email = Some(value.clone()),
                "default_project" => cfg.default_project = Some(value.clone()),
                "youtrack.url" => cfg.youtrack.url = Some(value.clone()),
                "youtrack.token" => cfg.youtrack.token = Some(value.clone()),
                "jira.url" => cfg.jira.url = Some(value.clone()),
                "jira.email" => cfg.jira.email = Some(value.clone()),
                "jira.token" => cfg.jira.token = Some(value.clone()),
                _ => unreachable!("Key validated above"),
            }

            cfg.save(&config_path)?;

            match format {
                cli::OutputFormat::Json => {
                    println!(r#"{{"success": true, "key": "{}", "value": "{}"}}"#, key, value);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("Set {} = {}", key.cyan().bold(), value.green());
                }
            }
            Ok(())
        }
        ConfigCommands::Get { key } => {
            // Validate key
            let valid_key = VALID_CONFIG_KEYS.iter().any(|(k, _, _)| *k == key);
            if !valid_key {
                return Err(anyhow::anyhow!(
                    "Invalid configuration key: '{}'\nRun 'track config keys' to see valid keys.",
                    key
                ));
            }

            let cfg = Config::load_local_track_toml()?.unwrap_or_default();

            let value: Option<&str> = match key.as_str() {
                "backend" => cfg.backend.as_deref(),
                "url" => cfg.url.as_deref(),
                "token" => cfg.token.as_deref(),
                "email" => cfg.email.as_deref(),
                "default_project" => cfg.default_project.as_deref(),
                "youtrack.url" => cfg.youtrack.url.as_deref(),
                "youtrack.token" => cfg.youtrack.token.as_deref(),
                "jira.url" => cfg.jira.url.as_deref(),
                "jira.email" => cfg.jira.email.as_deref(),
                "jira.token" => cfg.jira.token.as_deref(),
                _ => unreachable!("Key validated above"),
            };

            match format {
                cli::OutputFormat::Json => {
                    let output = serde_json::json!({
                        "key": key,
                        "value": value,
                        "is_set": value.is_some()
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    if let Some(v) = value {
                        // Mask tokens/secrets
                        let display_value = if key.contains("token") {
                            "(set - hidden)".to_string()
                        } else {
                            v.to_string()
                        };
                        println!("{} = {}", key.cyan(), display_value.green());
                    } else {
                        println!("{} is not set", key.cyan());
                    }
                }
            }
            Ok(())
        }
        ConfigCommands::Show => {
            let config = Config::load_local_track_toml()?;
            match format {
                cli::OutputFormat::Json => {
                    if let Some(cfg) = &config {
                        let output = serde_json::json!({
                            "backend": cfg.backend,
                            "default_project": cfg.default_project,
                            "url": cfg.url,
                            "has_token": cfg.token.is_some(),
                            "has_email": cfg.email.is_some(),
                            "youtrack": {
                                "url": cfg.youtrack.url,
                                "has_token": cfg.youtrack.token.is_some()
                            },
                            "jira": {
                                "url": cfg.jira.url,
                                "email": cfg.jira.email,
                                "has_token": cfg.jira.token.is_some()
                            }
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
                        let backend_name = cfg.backend.as_deref().unwrap_or("youtrack");
                        println!("  {}: {}", "Backend".dimmed(), backend_name.cyan().bold());
                        if let Some(url) = &cfg.url {
                            println!("  {}: {}", "URL".dimmed(), url.cyan());
                        }
                        if cfg.email.is_some() {
                            println!("  {}: {}", "Email".dimmed(), "(set)".green());
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

                        // Show backend-specific config if set
                        if !cfg.youtrack.is_empty() {
                            println!();
                            println!("  {}:", "[youtrack]".white().bold());
                            if let Some(url) = &cfg.youtrack.url {
                                println!("    {}: {}", "url".dimmed(), url.cyan());
                            }
                            if cfg.youtrack.token.is_some() {
                                println!("    {}: {}", "token".dimmed(), "(set)".green());
                            }
                        }

                        if !cfg.jira.is_empty() {
                            println!();
                            println!("  {}:", "[jira]".white().bold());
                            if let Some(url) = &cfg.jira.url {
                                println!("    {}: {}", "url".dimmed(), url.cyan());
                            }
                            if let Some(email) = &cfg.jira.email {
                                println!("    {}: {}", "email".dimmed(), email.cyan());
                            }
                            if cfg.jira.token.is_some() {
                                println!("    {}: {}", "token".dimmed(), "(set)".green());
                            }
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
            // Clear default_project and backend from .track.toml (keep url/token)
            let config_path = config::local_track_config_path()?;
            if let Some(mut cfg) = Config::load_local_track_toml()? {
                cfg.default_project = None;
                cfg.backend = None;
                cfg.save(&config_path)?;
                match format {
                    cli::OutputFormat::Json => {
                        println!(r#"{{"success": true, "message": "Configuration cleared"}}"#);
                    }
                    cli::OutputFormat::Text => {
                        use colored::Colorize;
                        println!("{}", "Default project and backend cleared.".green());
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
        ConfigCommands::Project { .. } | ConfigCommands::Test | ConfigCommands::Backend { .. } => {
            // This shouldn't be called - handled elsewhere
            unreachable!("Project/Test/Backend command should be handled elsewhere")
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
                    println!("{} Connected to {}", "✓".green().bold(), url.cyan());
                    println!(
                        "  {} projects accessible",
                        projects.len().to_string().white().bold()
                    );
                }
            }
            Ok(())
        }
        // These are handled elsewhere before API validation
        ConfigCommands::Show
        | ConfigCommands::Clear
        | ConfigCommands::Path
        | ConfigCommands::Keys
        | ConfigCommands::Set { .. }
        | ConfigCommands::Get { .. }
        | ConfigCommands::Backend { .. } => {
            unreachable!("Local config commands should be handled before API validation")
        }
    }
}

fn handle_init(
    url: &str,
    token: &str,
    project: Option<&str>,
    email: Option<&str>,
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

    // For Jira, require email
    let effective_email: Option<String> = match backend {
        Backend::Jira => {
            Some(
                email
                    .map(|e| e.to_string())
                    .or_else(|| std::env::var("JIRA_EMAIL").ok())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Jira requires email for authentication.\nUse --email flag or set JIRA_EMAIL environment variable."
                        )
                    })?,
            )
        }
        Backend::YouTrack => email.map(|e| e.to_string()),
    };

    // If project is specified, validate it against the server
    let validated_project: Option<(String, String)> = if let Some(proj) = project {
        // Create temporary client to validate project
        let projects: Vec<tracker_core::Project> = match backend {
            Backend::YouTrack => {
                let client = YouTrackClient::new(url, token);
                let tracker: &dyn IssueTracker = &client;
                tracker.list_projects().map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to connect to server or list projects: {}\nCheck your URL and token.",
                        e
                    )
                })?
            }
            Backend::Jira => {
                let client = JiraClient::new(url, effective_email.as_ref().unwrap(), token);
                let tracker: &dyn IssueTracker = &client;
                tracker.list_projects().map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to connect to server or list projects: {}\nCheck your URL, email, and token.",
                        e
                    )
                })?
            }
        };

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

    // Create backend name string for config
    let backend_str = match backend {
        Backend::YouTrack => "youtrack",
        Backend::Jira => "jira",
    };

    // Create config with backend and optional default project
    let config = Config {
        backend: Some(backend_str.to_string()),
        url: Some(url.to_string()),
        token: Some(token.to_string()),
        email: effective_email,
        default_project: validated_project.as_ref().map(|(_, name)| name.clone()),
        youtrack: Default::default(),
        jira: Default::default(),
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
                r#"{{"success": true, "backend": "{}", "config_path": "{}"{}}}"#,
                backend_str,
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
            println!("  {}: {}", "Backend".dimmed(), backend_str.cyan().bold());
            if let Some((_, name)) = &validated_project {
                println!("  {}: {}", "Default project".dimmed(), name.cyan().bold());
            }
            println!();
            println!(
                "{}",
                "You can now use track commands without --url, --token, and -b flags.".dimmed()
            );
        }
    }

    Ok(())
}

fn handle_open(id: Option<&str>, config: &Config, format: cli::OutputFormat) -> Result<()> {
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
                println!(
                    r#"{{"success": false, "url": "{}", "error": "Failed to open browser"}}"#,
                    url
                );
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

    // Record access for LRU tracking (same as issue get command)
    if let Ok(mut c) = cache::TrackerCache::load(None) {
        c.record_issue_access(&issue);
        let _ = c.save(None);
    }

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
