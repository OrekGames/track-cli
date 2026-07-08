mod cache;
mod cli;
mod color;
mod commands;
mod config;
mod output;

use anyhow::{Result, anyhow};
use clap::Parser;
use cli::{Backend, Cli, Commands};
use config::Config;
use github_backend::GitHubClient;
use gitlab_backend::GitLabClient;
use jira_backend::{ConfluenceClient, JiraClient};
use linear_backend::LinearClient;
use output::output_error;
use std::process::ExitCode;
use tracker_core::{IssueTracker, KnowledgeBase};
use tracker_mock::MockClient;
use youtrack_backend::YouTrackClient;

/// Debug builds of the command dispatch need more stack than Windows' 1 MiB
/// main-thread default, so the CLI runs on a thread with an explicit size.
const MAIN_STACK_SIZE: usize = 8 * 1024 * 1024;

fn main() -> ExitCode {
    std::thread::Builder::new()
        .stack_size(MAIN_STACK_SIZE)
        .spawn(cli_main)
        .expect("failed to spawn CLI thread")
        .join()
        // A panic on the CLI thread already printed its message; exit with
        // the same code a panicking main thread would.
        .unwrap_or(ExitCode::from(101))
}

fn cli_main() -> ExitCode {
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

    // Handle doctor command - audits one or many backends, so it doesn't fit
    // the single-client dispatch below
    if let Commands::Doctor {
        all_backends,
        project,
        write_check,
        strict,
    } = &cli.command
    {
        return commands::doctor::handle_doctor(
            &cli,
            commands::doctor::DoctorOptions {
                all_backends: *all_backends,
                project: project.as_deref(),
                write_check: write_check.is_some(),
                strict: *strict,
            },
        );
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
    let client = build_client(effective_backend, &config)?;
    run_with_client(
        client.issue_tracker(),
        client.knowledge_base(),
        &cli,
        &config,
    )
}

/// A constructed backend client, exposing the issue-tracker and knowledge-base
/// trait objects it implements. Jira is the only backend that splits the two
/// across separate clients (Jira + Confluence).
pub enum BackendClient {
    YouTrack(YouTrackClient),
    Jira {
        issues: JiraClient,
        confluence: ConfluenceClient,
    },
    GitHub(GitHubClient),
    GitLab(GitLabClient),
    Linear(LinearClient),
    Mock(MockClient),
}

impl BackendClient {
    pub fn issue_tracker(&self) -> &dyn IssueTracker {
        match self {
            BackendClient::YouTrack(c) => c,
            BackendClient::Jira { issues, .. } => issues,
            BackendClient::GitHub(c) => c,
            BackendClient::GitLab(c) => c,
            BackendClient::Linear(c) => c,
            BackendClient::Mock(c) => c,
        }
    }

    pub fn knowledge_base(&self) -> &dyn KnowledgeBase {
        match self {
            BackendClient::YouTrack(c) => c,
            BackendClient::Jira { confluence, .. } => confluence,
            BackendClient::GitHub(c) => c,
            BackendClient::GitLab(c) => c,
            BackendClient::Linear(c) => c,
            BackendClient::Mock(c) => c,
        }
    }
}

/// Build a client for `backend` from an already backend-collapsed [`Config`]
/// (i.e. one produced by `Config::load(_, backend)`).
///
/// The config should normally be validated with `config.validate(backend)`
/// first; missing settings surface as errors rather than panics so callers
/// like `track doctor` can probe multiple backends safely.
pub fn build_client(backend: Backend, config: &Config) -> Result<BackendClient> {
    let missing = |what: &str| anyhow!("{} not configured", what);
    match backend {
        Backend::YouTrack => {
            let url = config.url.as_ref().ok_or_else(|| missing("YouTrack URL"))?;
            let token = config
                .token
                .as_ref()
                .ok_or_else(|| missing("YouTrack token"))?;
            let client = YouTrackClient::new(url, token)
                .with_link_mappings(config.youtrack.link_mappings.clone());
            Ok(BackendClient::YouTrack(client))
        }
        Backend::Jira => {
            let url = config.url.as_ref().ok_or_else(|| missing("Jira URL"))?;
            let email = config.email.as_ref().ok_or_else(|| missing("Jira email"))?;
            let token = config.token.as_ref().ok_or_else(|| missing("Jira token"))?;

            let issues = JiraClient::new(url, email, token)
                .with_link_mappings(config.jira.link_mappings.clone());
            let confluence = ConfluenceClient::new(url, email, token);
            Ok(BackendClient::Jira { issues, confluence })
        }
        Backend::GitHub => {
            let owner = config
                .github
                .owner
                .as_deref()
                .ok_or_else(|| missing("GitHub owner"))?;
            let repo = config
                .github
                .repo
                .as_deref()
                .ok_or_else(|| missing("GitHub repo"))?;
            let token = config
                .token
                .as_ref()
                .ok_or_else(|| missing("GitHub token"))?;
            let client = if let Some(api_url) = config.url.as_deref() {
                GitHubClient::with_base_url(api_url, owner, repo, token)
            } else {
                GitHubClient::new(owner, repo, token)
            };
            Ok(BackendClient::GitHub(client))
        }
        Backend::GitLab => {
            let base_url = config.url.as_ref().ok_or_else(|| missing("GitLab URL"))?;
            let token = config
                .token
                .as_ref()
                .ok_or_else(|| missing("GitLab token"))?;
            let project_id = config.gitlab.project_id.as_deref();
            let client = GitLabClient::new(base_url, token, project_id)
                .with_link_mappings(config.gitlab.link_mappings.clone());
            Ok(BackendClient::GitLab(client))
        }
        Backend::Linear => {
            let token = config
                .token
                .as_ref()
                .ok_or_else(|| missing("Linear token"))?;
            let api_url = config
                .linear
                .api_url
                .as_deref()
                .unwrap_or("https://api.linear.app/graphql");
            let default_team = config
                .linear
                .default_team
                .clone()
                .or_else(|| config.default_project.clone());
            let client = LinearClient::with_base_url(api_url, token)
                .with_defaults(default_team, config.linear.default_linear_project.clone())
                .with_link_mappings(config.linear.link_mappings.clone());
            Ok(BackendClient::Linear(client))
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
        Commands::Apply {
            plan,
            dry_run,
            validate,
            resume,
            allow_delete,
        } => commands::apply::handle_apply(
            issue_client,
            commands::apply::ApplyOptions {
                plan_path: plan,
                dry_run: *dry_run,
                validate: *validate,
                resume_path: resume.as_deref(),
                allow_delete: *allow_delete,
                format: cli.format,
                default_project: config.default_project.as_deref(),
            },
        ),
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
        Commands::Doctor { .. } => {
            unreachable!("Doctor command should be handled before API validation")
        }
    }
}
