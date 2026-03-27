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
        return commands::eval::handle_eval(action, cli.format);
    }

    // Handle init command - creates config, no existing auth needed
    if let Commands::Init {
        url,
        token,
        project,
        backend,
        email,
        skills,
        global,
    } = &cli.command
    {
        return commands::init::handle_init(
            url.as_deref(),
            token.as_deref(),
            project.as_deref(),
            email.as_deref(),
            cli.format,
            *backend,
            *skills,
            *global,
        );
    }

    // Handle config commands that don't need API connection
    if let Commands::Config { action } = &cli.command {
        use cli::ConfigCommands;
        match action {
            ConfigCommands::Show
            | ConfigCommands::Clear { .. }
            | ConfigCommands::Path
            | ConfigCommands::Keys
            | ConfigCommands::Set { .. }
            | ConfigCommands::Get { .. } => {
                return commands::config::handle_config_local(action, cli.format);
            }
            ConfigCommands::Backend { backend } => {
                return commands::config::handle_config_backend(*backend, cli.format);
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
            if !commands::open::is_issue_id(cmd) {
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

    // Determine effective backend: CLI flag > config chain (project > global > env) > default
    let effective_backend = cli.backend.unwrap_or_else(config::resolve_backend);

    let mut config = Config::load(cli.config.clone(), effective_backend)?;
    config.merge_with_cli(cli.url.clone(), cli.token.clone());

    // Check if mock mode is enabled (before config validation, since mock mode
    // doesn't need real backend credentials)
    if let Some(mock_dir) = tracker_mock::get_mock_dir() {
        let client = MockClient::new(&mock_dir)
            .map_err(|e| anyhow::anyhow!("Failed to initialize mock client: {}", e))?;
        return run_with_client(&client, &client, &cli, &config);
    }

    config.validate(effective_backend)?;

    // Create the appropriate backend client
    match effective_backend {
        Backend::YouTrack => {
            let client =
                YouTrackClient::new(config.url.as_ref().unwrap(), config.token.as_ref().unwrap())
                    .with_link_mappings(config.youtrack.link_mappings.clone());
            run_with_client(&client, &client, &cli, &config)
        }
        Backend::Jira => {
            let url = config.url.as_ref().unwrap();
            let email = config.email.as_ref().unwrap();
            let token = config.token.as_ref().unwrap();

            let client = JiraClient::new(url, email, token)
                .with_link_mappings(config.jira.link_mappings.clone());
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
            let client = GitLabClient::new(base_url, token, project_id)
                .with_link_mappings(config.gitlab.link_mappings.clone());
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
            cli.verbose,
        ),
        Commands::Project { action } => {
            commands::project::handle_project(issue_client, action, cli.format)
        }
        Commands::Tags { action } => commands::tags::handle_tags(issue_client, action, cli.format),
        Commands::Cache { action } => {
            let backend = cli.backend.unwrap_or_else(|| config.get_backend());
            commands::cache::handle_cache(
                issue_client,
                Some(kb_client),
                action,
                cli.format,
                backend,
                config,
            )
        }
        Commands::Config { action } => {
            commands::config::handle_config(issue_client, action, cli.format, config)
        }
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
            // Log context command for eval scoring when in mock mode
            if let Some(mock_dir) = tracker_mock::get_mock_dir() {
                let mut args: Vec<(&str, &str)> = Vec::new();
                if let Some(p) = project.as_deref() {
                    args.push(("project", p));
                }
                if *refresh {
                    args.push(("refresh", "true"));
                }
                tracker_mock::log_cli_command(&mock_dir, "context", &args);
            }

            let backend = cli.backend.unwrap_or_else(|| config.get_backend());
            let backend_type = backend.to_string();
            commands::context::handle_context(
                issue_client,
                Some(kb_client),
                project.as_deref(),
                *refresh,
                *include_issues,
                *issue_limit,
                cli.format,
                &backend_type,
                config.url.as_deref().unwrap_or("unknown"),
                config.default_project.as_deref(),
            )
        }
        Commands::Completions { .. } => {
            unreachable!("Completions command should be handled before API validation")
        }
        Commands::Init { .. } => {
            unreachable!("Init command should be handled before API validation")
        }
        Commands::Open { id } => commands::open::handle_open(id.as_deref(), config, cli.format),
        Commands::External(args) => {
            commands::open::handle_issue_shortcut(issue_client, args, cli.format)
        }
        Commands::Eval { .. } => {
            unreachable!("Eval command should be handled before API validation")
        }
    }
}
