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
use github_backend::GitHubClient;
use gitlab_backend::GitLabClient;
use jira_backend::{ConfluenceClient, JiraClient};
use output::output_error;
use std::process::ExitCode;
use tracker_core::{IssueTracker, KnowledgeBase};
use tracker_mock::MockClient;
use youtrack_backend::YouTrackClient;

/// Embedded agent guide content - written to project directory during `track init`
const AGENT_GUIDE: &str = include_str!("../../../docs/agent_guide.md");

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

    // Handle eval command - no API needed (uses mock system)
    if let Commands::Eval { action } = &cli.command {
        return handle_eval(action, cli.format);
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

    // Check if mock mode is enabled
    if let Some(mock_dir) = tracker_mock::get_mock_dir() {
        let client = MockClient::new(&mock_dir)
            .map_err(|e| anyhow::anyhow!("Failed to initialize mock client: {}", e))?;
        return run_with_client(&client, &client, &cli, &config);
    }

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
        Backend::GitHub => {
            let owner = config
                .github
                .owner
                .as_deref()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "GitHub owner not configured. Set via 'track config set github.owner <OWNER>' or GITHUB_OWNER env var"
                    )
                })?;
            let repo = config
                .github
                .repo
                .as_deref()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "GitHub repo not configured. Set via 'track config set github.repo <REPO>' or GITHUB_REPO env var"
                    )
                })?;
            let token = config.token.as_ref().unwrap();
            let client = if let Some(api_url) = config.url.as_deref() {
                GitHubClient::with_base_url(api_url, owner, repo, token)
            } else {
                GitHubClient::new(owner, repo, token)
            };
            run_with_client(&client, &client, &cli, &config)
        }
        Backend::GitLab => {
            let base_url = config.url.as_ref().unwrap();
            let token = config.token.as_ref().unwrap();
            let project_id = config.gitlab.project_id.as_deref();
            let client = GitLabClient::new(base_url, token, project_id);
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
            handle_cache(
                issue_client,
                Some(kb_client),
                action,
                cli.format,
                backend,
                config,
            )
        }
        Commands::Config { action } => handle_config(issue_client, action, cli.format, config),
        Commands::Article { action } => {
            commands::article::handle_article(issue_client, kb_client, action, cli.format)
        }
        Commands::Field { action } => {
            commands::field::handle_field(issue_client, action, cli.format)
        }
        Commands::Bundle { action } => {
            commands::bundle::handle_bundle(issue_client, action, cli.format)
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
                Backend::GitHub => "github",
                Backend::GitLab => "gitlab",
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
        Commands::Eval { .. } => {
            // Eval is handled before config loading - should never reach here
            unreachable!("Eval command should be handled before API validation")
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
                            let age_seconds =
                                existing_cache.age().map(|a| a.num_seconds()).unwrap_or(0);
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
                Backend::GitHub => "github",
                Backend::GitLab => "gitlab",
            };
            let base_url = config.url.as_deref().unwrap_or("unknown");
            let default_project = config.default_project.as_deref();

            let cache = cache::TrackerCache::refresh_with_articles(
                client,
                kb_client,
                backend_type,
                base_url,
                default_project,
            )?;
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
        Backend::GitHub => "github",
        Backend::GitLab => "gitlab",
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
    (
        "backend",
        "youtrack | jira | github | gitlab",
        "Default backend to use",
    ),
    ("url", "string", "Tracker instance URL"),
    (
        "token",
        "string",
        "API token (YouTrack permanent token or Jira API token)",
    ),
    (
        "email",
        "string",
        "Email for authentication (required for Jira)",
    ),
    (
        "default_project",
        "string",
        "Default project shortName (e.g., \"PROJ\")",
    ),
    (
        "youtrack.url",
        "string",
        "YouTrack-specific URL (overrides 'url' when backend=youtrack)",
    ),
    ("youtrack.token", "string", "YouTrack-specific token"),
    (
        "jira.url",
        "string",
        "Jira-specific URL (overrides 'url' when backend=jira)",
    ),
    ("jira.email", "string", "Jira-specific email"),
    ("jira.token", "string", "Jira-specific token"),
    ("github.token", "string", "GitHub personal access token"),
    (
        "github.owner",
        "string",
        "GitHub repository owner (user or organization)",
    ),
    ("github.repo", "string", "GitHub repository name"),
    (
        "github.api_url",
        "string",
        "GitHub API URL (defaults to https://api.github.com)",
    ),
    ("gitlab.token", "string", "GitLab personal access token"),
    (
        "gitlab.url",
        "string",
        "GitLab instance URL (e.g., https://gitlab.com)",
    ),
    ("gitlab.project_id", "string", "GitLab numeric project ID"),
    ("gitlab.namespace", "string", "GitLab namespace/group path"),
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
                    println!(
                        r#"
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
"#
                    );
                    println!("{}:", "Usage".white().bold());
                    println!(
                        "  Set a value:  {}",
                        "track config set <key> <value>".cyan()
                    );
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
                    let valid_backends = [
                        "youtrack", "yt", "jira", "j", "github", "gh", "gitlab", "gl",
                    ];
                    if !valid_backends.contains(&value.as_str()) {
                        return Err(anyhow::anyhow!(
                            "Invalid backend value: '{}'. Use 'youtrack', 'jira', 'github', or 'gitlab'.",
                            value
                        ));
                    }
                    let normalized = match value.as_str() {
                        "yt" => "youtrack",
                        "j" => "jira",
                        "gh" => "github",
                        "gl" => "gitlab",
                        other => other,
                    };
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
                "github.token" => cfg.github.token = Some(value.clone()),
                "github.owner" => cfg.github.owner = Some(value.clone()),
                "github.repo" => cfg.github.repo = Some(value.clone()),
                "github.api_url" => cfg.github.api_url = Some(value.clone()),
                "gitlab.token" => cfg.gitlab.token = Some(value.clone()),
                "gitlab.url" => cfg.gitlab.url = Some(value.clone()),
                "gitlab.project_id" => cfg.gitlab.project_id = Some(value.clone()),
                "gitlab.namespace" => cfg.gitlab.namespace = Some(value.clone()),
                _ => unreachable!("Key validated above"),
            }

            cfg.save(&config_path)?;

            match format {
                cli::OutputFormat::Json => {
                    println!(
                        r#"{{"success": true, "key": "{}", "value": "{}"}}"#,
                        key, value
                    );
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
                "github.token" => cfg.github.token.as_deref(),
                "github.owner" => cfg.github.owner.as_deref(),
                "github.repo" => cfg.github.repo.as_deref(),
                "github.api_url" => cfg.github.api_url.as_deref(),
                "gitlab.token" => cfg.gitlab.token.as_deref(),
                "gitlab.url" => cfg.gitlab.url.as_deref(),
                "gitlab.project_id" => cfg.gitlab.project_id.as_deref(),
                "gitlab.namespace" => cfg.gitlab.namespace.as_deref(),
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
                            },
                            "github": {
                                "owner": cfg.github.owner,
                                "repo": cfg.github.repo,
                                "api_url": cfg.github.api_url,
                                "has_token": cfg.github.token.is_some()
                            },
                            "gitlab": {
                                "url": cfg.gitlab.url,
                                "project_id": cfg.gitlab.project_id,
                                "namespace": cfg.gitlab.namespace,
                                "has_token": cfg.gitlab.token.is_some()
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

                        if !cfg.github.is_empty() {
                            println!();
                            println!("  {}:", "[github]".white().bold());
                            if let Some(owner) = &cfg.github.owner {
                                println!("    {}: {}", "owner".dimmed(), owner.cyan());
                            }
                            if let Some(repo) = &cfg.github.repo {
                                println!("    {}: {}", "repo".dimmed(), repo.cyan());
                            }
                            if let Some(api_url) = &cfg.github.api_url {
                                println!("    {}: {}", "api_url".dimmed(), api_url.cyan());
                            }
                            if cfg.github.token.is_some() {
                                println!("    {}: {}", "token".dimmed(), "(set)".green());
                            }
                        }

                        if !cfg.gitlab.is_empty() {
                            println!();
                            println!("  {}:", "[gitlab]".white().bold());
                            if let Some(url) = &cfg.gitlab.url {
                                println!("    {}: {}", "url".dimmed(), url.cyan());
                            }
                            if let Some(project_id) = &cfg.gitlab.project_id {
                                println!("    {}: {}", "project_id".dimmed(), project_id.cyan());
                            }
                            if let Some(namespace) = &cfg.gitlab.namespace {
                                println!("    {}: {}", "namespace".dimmed(), namespace.cyan());
                            }
                            if cfg.gitlab.token.is_some() {
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
        Backend::YouTrack | Backend::GitHub | Backend::GitLab => email.map(|e| e.to_string()),
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
            Backend::GitHub => {
                let client = GitHubClient::new("", "", token);
                // For GitHub, we try to list repos to validate the token
                let tracker: &dyn IssueTracker = &client;
                tracker.list_projects().map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to connect to GitHub or list repositories: {}\nCheck your token.",
                        e
                    )
                })?
            }
            Backend::GitLab => {
                let client = GitLabClient::new(url, token, None);
                let tracker: &dyn IssueTracker = &client;
                tracker.list_projects().map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to connect to GitLab or list projects: {}\nCheck your URL and token.",
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
        Backend::GitHub => "github",
        Backend::GitLab => "gitlab",
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
        github: Default::default(),
        gitlab: Default::default(),
    };

    config.save(&config_path)?;

    // Write agent guide to the same directory as the config
    let guide_path = config_path
        .parent()
        .map(|p| p.join("AGENT_GUIDE.md"))
        .unwrap_or_else(|| std::path::PathBuf::from("AGENT_GUIDE.md"));
    std::fs::write(&guide_path, AGENT_GUIDE)?;

    match format {
        cli::OutputFormat::Json => {
            let project_json = if let Some((_, name)) = &validated_project {
                format!(r#", "default_project": "{}""#, name)
            } else {
                String::new()
            };
            println!(
                r#"{{"success": true, "backend": "{}", "config_path": "{}", "guide_path": "{}"{}}}"#,
                backend_str,
                config_path.display(),
                guide_path.display(),
                project_json
            );
        }
        cli::OutputFormat::Text => {
            println!(
                "{} {}",
                "Created config file:".green(),
                config_path.display()
            );
            println!(
                "{} {}",
                "Created agent guide:".green(),
                guide_path.display()
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
            println!(
                "{}",
                "AI agents can reference AGENT_GUIDE.md for CLI usage patterns.".dimmed()
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

/// Print evaluation result in the specified format
fn print_eval_result(
    result: &tracker_mock::EvaluationResult,
    format: cli::OutputFormat,
) -> Result<()> {
    match format {
        cli::OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&result)?;
            println!("{}", json);
        }
        cli::OutputFormat::Text => {
            use colored::Colorize;

            // Header
            println!(
                "\n{}: {}",
                "Scenario".white().bold(),
                result.scenario_name.cyan()
            );
            println!("{}", "=".repeat(60));

            // Overall result
            let status = if result.success {
                "PASS".green().bold()
            } else {
                "FAIL".red().bold()
            };
            println!("\n{}: {}", "Result".white().bold(), status);
            println!(
                "{}: {}/{} ({:.0}%)",
                "Score".white().bold(),
                result.score,
                result.max_score,
                result.score_percent
            );
            println!(
                "{}: {} (optimal: {})",
                "Commands".white().bold(),
                result.total_calls,
                result
                    .optimal_calls
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            );
            println!("{}: {:?}", "Efficiency".white().bold(), result.efficiency);

            // Outcomes
            println!("\n{}:", "Expected Outcomes".white().bold());
            for outcome in &result.outcomes {
                let icon = if outcome.achieved {
                    "✓".green()
                } else {
                    "✗".red()
                };
                println!("  {} {}", icon, outcome.name);
                if !outcome.achieved {
                    println!("    Expected: {}", outcome.expected.dimmed());
                    println!("    Actual:   {}", outcome.actual.yellow());
                }
            }

            // Score breakdown
            if !result.score_breakdown.penalties.is_empty()
                || !result.score_breakdown.bonuses.is_empty()
            {
                println!("\n{}:", "Score Breakdown".white().bold());
                println!("  Base: {}", result.score_breakdown.base);

                for bonus in &result.score_breakdown.bonuses {
                    println!(
                        "  {} {} (x{})",
                        format!("+{}", bonus.points).green(),
                        bonus.reason,
                        bonus.count
                    );
                }

                for penalty in &result.score_breakdown.penalties {
                    println!(
                        "  {} {} (x{})",
                        penalty.points.to_string().red(),
                        penalty.reason,
                        penalty.count
                    );
                }
            }

            // Suggestions
            if !result.suggestions.is_empty() {
                println!("\n{}:", "Suggestions".white().bold());
                for suggestion in &result.suggestions {
                    println!("  • {}", suggestion.yellow());
                }
            }

            println!();
        }
    }
    Ok(())
}

fn handle_eval(action: &cli::EvalCommands, format: cli::OutputFormat) -> Result<()> {
    use cli::EvalCommands;
    use tracker_mock::{EvaluationResult, Evaluator, MockClient, Scenario};

    match action {
        EvalCommands::Run {
            scenario,
            min_score,
            strict,
        } => {
            // Load scenario and call log
            let scenario_data = Scenario::load_from_dir(scenario)
                .map_err(|e| anyhow::anyhow!("Failed to load scenario: {}", e))?;

            let client = MockClient::new(scenario)
                .map_err(|e| anyhow::anyhow!("Failed to load mock client: {}", e))?;

            let calls = client
                .read_call_log()
                .map_err(|e| anyhow::anyhow!("Failed to read call log: {}", e))?;

            if calls.is_empty() {
                return Err(anyhow::anyhow!(
                    "Call log is empty. Run commands with TRACK_MOCK_DIR={} first.",
                    scenario.display()
                ));
            }

            // Run evaluation
            let evaluator = Evaluator::new(scenario_data);
            let result = evaluator.evaluate(&calls);

            // Print results
            print_eval_result(&result, format)?;

            // Check CI thresholds
            let score_ok = result.score_percent >= *min_score as f64;
            let strict_ok = !*strict || result.success;

            if !score_ok {
                return Err(anyhow::anyhow!(
                    "Score {:.0}% is below minimum required {}%",
                    result.score_percent,
                    min_score
                ));
            }

            if !strict_ok {
                return Err(anyhow::anyhow!(
                    "Not all expected outcomes were achieved (--strict mode)"
                ));
            }

            Ok(())
        }

        EvalCommands::RunAll {
            path,
            min_score,
            fail_fast,
        } => {
            let entries = std::fs::read_dir(path)
                .map_err(|e| anyhow::anyhow!("Failed to read scenarios directory: {}", e))?;

            let mut scenarios: Vec<(std::path::PathBuf, Scenario)> = Vec::new();
            for entry in entries.flatten() {
                let scenario_file = entry.path().join("scenario.toml");
                if scenario_file.exists() {
                    if let Ok(scenario) = Scenario::load(&scenario_file) {
                        scenarios.push((entry.path(), scenario));
                    }
                }
            }

            if scenarios.is_empty() {
                return Err(anyhow::anyhow!("No scenarios found in {}", path.display()));
            }

            let mut results: Vec<(String, EvaluationResult, bool)> = Vec::new();
            let mut all_passed = true;

            for (scenario_path, scenario_data) in &scenarios {
                let client = match MockClient::new(scenario_path) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Warning: Failed to load {}: {}", scenario_path.display(), e);
                        continue;
                    }
                };

                let calls = match client.read_call_log() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to read call log for {}: {}",
                            scenario_data.scenario.name, e
                        );
                        continue;
                    }
                };

                if calls.is_empty() {
                    match format {
                        cli::OutputFormat::Json => {}
                        cli::OutputFormat::Text => {
                            use colored::Colorize;
                            println!(
                                "{}: {} - {}",
                                "SKIP".yellow(),
                                scenario_data.scenario.name,
                                "empty call log".dimmed()
                            );
                        }
                    }
                    continue;
                }

                let evaluator = Evaluator::new(scenario_data.clone());
                let result = evaluator.evaluate(&calls);

                let passed = result.score_percent >= *min_score as f64 && result.success;
                if !passed {
                    all_passed = false;
                }

                results.push((scenario_data.scenario.name.clone(), result, passed));

                if !passed && *fail_fast {
                    break;
                }
            }

            // Print summary
            match format {
                cli::OutputFormat::Json => {
                    let summary: Vec<_> = results
                        .iter()
                        .map(|(name, result, passed)| {
                            serde_json::json!({
                                "scenario": name,
                                "passed": passed,
                                "score": result.score,
                                "score_percent": result.score_percent,
                                "total_calls": result.total_calls,
                                "success": result.success,
                            })
                        })
                        .collect();
                    let output = serde_json::json!({
                        "all_passed": all_passed,
                        "total": results.len(),
                        "passed": results.iter().filter(|(_, _, p)| *p).count(),
                        "failed": results.iter().filter(|(_, _, p)| !*p).count(),
                        "results": summary,
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    println!("\n{}", "Evaluation Results".white().bold());
                    println!("{}", "=".repeat(60));

                    for (name, result, passed) in &results {
                        let status = if *passed {
                            "PASS".green()
                        } else {
                            "FAIL".red()
                        };
                        println!(
                            "  {} {} - {:.0}% ({} calls)",
                            status,
                            name.cyan(),
                            result.score_percent,
                            result.total_calls
                        );
                    }

                    println!("{}", "-".repeat(60));
                    let passed_count = results.iter().filter(|(_, _, p)| *p).count();
                    let total = results.len();

                    if all_passed {
                        println!(
                            "  {} {}/{} scenarios passed",
                            "✓".green().bold(),
                            passed_count,
                            total
                        );
                    } else {
                        println!(
                            "  {} {}/{} scenarios passed",
                            "✗".red().bold(),
                            passed_count,
                            total
                        );
                    }
                    println!();
                }
            }

            if !all_passed {
                return Err(anyhow::anyhow!("One or more scenarios failed"));
            }

            Ok(())
        }

        EvalCommands::List { path } => {
            let entries = std::fs::read_dir(path)
                .map_err(|e| anyhow::anyhow!("Failed to read scenarios directory: {}", e))?;

            let mut scenarios = Vec::new();
            for entry in entries.flatten() {
                let scenario_file = entry.path().join("scenario.toml");
                if scenario_file.exists() {
                    if let Ok(scenario) = Scenario::load(&scenario_file) {
                        scenarios.push((entry.path(), scenario));
                    }
                }
            }

            match format {
                cli::OutputFormat::Json => {
                    let list: Vec<_> = scenarios
                        .iter()
                        .map(|(path, s)| {
                            serde_json::json!({
                                "path": path,
                                "name": s.scenario.name,
                                "description": s.scenario.description,
                                "backend": s.scenario.backend,
                                "difficulty": s.scenario.difficulty,
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&list)?);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    if scenarios.is_empty() {
                        println!("No scenarios found in {}", path.display());
                        return Ok(());
                    }

                    println!("{}:", "Available Scenarios".white().bold());
                    for (scenario_path, scenario) in &scenarios {
                        println!(
                            "\n  {} ({})",
                            scenario.scenario.name.cyan().bold(),
                            scenario.scenario.difficulty.dimmed()
                        );
                        println!("    {}", scenario.scenario.description);
                        println!("    Path: {}", scenario_path.display().to_string().dimmed());
                        if !scenario.scenario.tags.is_empty() {
                            println!("    Tags: {}", scenario.scenario.tags.join(", ").magenta());
                        }
                    }
                    println!();
                }
            }
            Ok(())
        }

        EvalCommands::Show { scenario } => {
            let scenario_data = Scenario::load_from_dir(scenario)
                .map_err(|e| anyhow::anyhow!("Failed to load scenario: {}", e))?;

            match format {
                cli::OutputFormat::Json => {
                    // Return the full scenario as JSON
                    println!("{}", serde_json::to_string_pretty(&scenario_data)?);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    println!(
                        "\n{}: {}",
                        "Scenario".white().bold(),
                        scenario_data.scenario.name.cyan().bold()
                    );
                    println!("{}", "=".repeat(60));
                    println!("{}", scenario_data.scenario.description);

                    println!("\n{}:", "Setup".white().bold());
                    println!("  Backend: {}", scenario_data.scenario.backend.cyan());
                    println!(
                        "  Difficulty: {}",
                        scenario_data.scenario.difficulty.yellow()
                    );
                    if let Some(project) = &scenario_data.setup.default_project {
                        println!("  Default Project: {}", project.cyan());
                    }
                    if scenario_data.setup.cache_available {
                        println!("  Cache: {}", "available".green());
                    }

                    println!("\n{}:", "Agent Prompt".white().bold());
                    for line in scenario_data.setup.prompt.lines() {
                        println!("  {}", line);
                    }

                    if let Some(context) = &scenario_data.setup.context {
                        println!("\n{}:", "Additional Context".white().bold());
                        for line in context.lines() {
                            println!("  {}", line.dimmed());
                        }
                    }

                    println!("\n{}:", "Expected Outcomes".white().bold());
                    for name in scenario_data.expected_outcomes.keys() {
                        println!("  • {}", name);
                    }

                    println!("\n{}:", "Scoring".white().bold());
                    if let Some(min) = scenario_data.scoring.min_commands {
                        println!("  Min commands: {}", min);
                    }
                    if let Some(opt) = scenario_data.scoring.optimal_commands {
                        println!("  Optimal commands: {}", opt.to_string().green());
                    }
                    if let Some(max) = scenario_data.scoring.max_commands {
                        println!("  Max commands: {}", max);
                    }
                    println!("  Base score: {}", scenario_data.scoring.base_score);

                    println!("\n{}:", "Usage".white().bold());
                    println!(
                        "  1. Clear log: {}",
                        format!("track eval clear {}", scenario.display()).cyan()
                    );
                    println!(
                        "  2. Run agent: {}",
                        format!("TRACK_MOCK_DIR={} <agent commands>", scenario.display()).cyan()
                    );
                    println!(
                        "  3. Evaluate:  {}",
                        format!("track eval run {}", scenario.display()).cyan()
                    );
                    println!();
                }
            }
            Ok(())
        }

        EvalCommands::Clear { scenario } => {
            let log_path = scenario.join("call_log.jsonl");

            if log_path.exists() {
                std::fs::write(&log_path, "")?;
            }

            match format {
                cli::OutputFormat::Json => {
                    println!(r#"{{"success": true, "path": "{}"}}"#, log_path.display());
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{} {}", "Cleared:".green(), log_path.display());
                }
            }
            Ok(())
        }

        EvalCommands::ClearAll { path } => {
            let entries = std::fs::read_dir(path)
                .map_err(|e| anyhow::anyhow!("Failed to read scenarios directory: {}", e))?;

            let mut cleared = 0;
            for entry in entries.flatten() {
                let log_path = entry.path().join("call_log.jsonl");
                if log_path.exists() {
                    std::fs::write(&log_path, "")?;
                    cleared += 1;
                }
            }

            match format {
                cli::OutputFormat::Json => {
                    println!(r#"{{"success": true, "cleared": {}}}"#, cleared);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{} {} scenario call logs", "Cleared:".green(), cleared);
                }
            }
            Ok(())
        }

        EvalCommands::Status => {
            let mock_dir = tracker_mock::get_mock_dir();
            let is_enabled = mock_dir.is_some();

            match format {
                cli::OutputFormat::Json => {
                    let status = serde_json::json!({
                        "mock_enabled": is_enabled,
                        "mock_dir": mock_dir,
                        "env_var": tracker_mock::MOCK_DIR_ENV,
                    });
                    println!("{}", serde_json::to_string_pretty(&status)?);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    println!("{}:", "Mock System Status".white().bold());
                    println!(
                        "  Environment variable: {}",
                        tracker_mock::MOCK_DIR_ENV.cyan()
                    );

                    if is_enabled {
                        println!("  Status: {}", "ENABLED".green().bold());
                        println!("  Mock directory: {}", mock_dir.unwrap().display());
                        println!(
                            "\n  {} All track commands will use mock responses.",
                            "Note:".yellow()
                        );
                    } else {
                        println!("  Status: {}", "disabled".dimmed());
                        println!(
                            "\n  To enable: {}",
                            "export TRACK_MOCK_DIR=./fixtures/scenarios/<name>".cyan()
                        );
                    }
                }
            }
            Ok(())
        }
    }
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
