mod cache;
mod cli;
mod color;
mod commands;
mod config;
mod errors;
mod fsutil;
mod lookup;
mod output;
mod token_guard;

use anyhow::{Result, anyhow};
use cli::{Backend, Cli, CliFormat, Commands};
use config::Config;
use github_backend::GitHubClient;
use gitlab_backend::GitLabClient;
use jira_backend::{ConfluenceClient, JiraClient};
use linear_backend::LinearClient;
use output::{output_error, parse_select_paths};
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

/// Whether the process was started as `track prime --hook[=AGENT]`, judged
/// from the raw arguments so it still holds when they do not parse.
fn is_prime_hook_invocation<I: IntoIterator<Item = String>>(args: I) -> bool {
    let args: Vec<String> = args.into_iter().skip(1).collect();
    args.iter().any(|arg| arg == "prime")
        && args
            .iter()
            .any(|arg| arg == "--hook" || arg.starts_with("--hook="))
}

/// Names the error format when `--error-format` does not.
const ERROR_FORMAT_ENV: &str = "TRACK_ERROR_FORMAT";

/// The value of the environment variable `var` as one of `T`'s variants.
/// An empty or unrecognised value counts as unset: a hook may leave a
/// variable set but empty, and that must not change what a command does.
fn value_enum_from_env<T: clap::ValueEnum>(var: &str) -> Option<T> {
    value_enum_from(std::env::var(var).ok().as_deref())
}

/// `value` as one of `T`'s variants, ignoring case and surrounding space;
/// `None` when it is absent, empty or names no variant.
fn value_enum_from<T: clap::ValueEnum>(value: Option<&str>) -> Option<T> {
    T::from_str(value?.trim(), true).ok()
}

fn cli_main() -> ExitCode {
    // A session hook runs in every project, configured for track or not, and
    // whatever it prints lands in an agent's context. It must stay silent and
    // succeed even when the arguments or environment do not parse.
    let quiet_hook = is_prime_hook_invocation(std::env::args());
    if quiet_hook {
        commands::prime::prepare_hook();
    }
    let (mut cli, matches) = match Cli::try_parse_args() {
        Ok(parsed) => parsed,
        Err(_) if quiet_hook => {
            commands::prime::print_empty_hook_output();
            return ExitCode::SUCCESS;
        }
        Err(e) => e.exit(),
    };
    // `TRACK_OUTPUT` picks the format when `-o` does not. Read here, not by
    // clap: a hook may leave it set but empty, and that must mean "unset".
    if matches.value_source("format") != Some(clap::parser::ValueSource::CommandLine)
        && let Some(format) = value_enum_from_env::<CliFormat>(output::OUTPUT_ENV)
    {
        cli.format = format;
    }
    // Errors follow the output: JSON output gets JSON errors, unless the
    // flag or `TRACK_ERROR_FORMAT` says otherwise.
    if matches.value_source("error_format") != Some(clap::parser::ValueSource::CommandLine) {
        let from_env = value_enum_from_env::<errors::ErrorFormat>(ERROR_FORMAT_ENV);
        cli.error_format = from_env.unwrap_or(if cli.format.output() == cli::OutputFormat::Json {
            errors::ErrorFormat::Json
        } else {
            errors::ErrorFormat::Text
        });
    }
    let error_format = cli.error_format;

    // Initialize color mode based on CLI flag and environment. Agent output
    // is never coloured: escape codes are noise to a model.
    if cli.format.is_agent() {
        color::init(cli::ColorChoice::Never);
    } else {
        color::init(cli.color);
    }
    output::set_agent_mode(cli.format.is_agent());

    if let Err(e) = run(cli) {
        if quiet_hook {
            commands::prime::print_empty_hook_output();
            return ExitCode::SUCCESS;
        }
        return ExitCode::from(output_error(&e, error_format).exit_code);
    }

    ExitCode::SUCCESS
}

/// What the configured credentials point at, as the cache records it: a
/// cache answers lookups only for the instance it was written for.
fn cache_scope(backend: Backend, config: &Config) -> String {
    if let Some(mock_dir) = tracker_mock::get_mock_dir() {
        let dir = std::fs::canonicalize(&mock_dir).unwrap_or(mock_dir);
        return format!("mock:{}", dir.display());
    }
    let endpoint = config.token_endpoint(backend).unwrap_or_default();
    // GitHub and GitLab clients are bound to one repository or project.
    let bound_to = match backend {
        Backend::GitHub => format!(
            "{}/{}",
            config.github.owner.as_deref().unwrap_or_default(),
            config.github.repo.as_deref().unwrap_or_default()
        ),
        Backend::GitLab => config.gitlab.project_id.clone().unwrap_or_default(),
        _ => String::new(),
    };
    format!("{}|{}", endpoint.trim_end_matches('/'), bound_to)
}

/// `-o jsonl` renders the document `-o json` prints, so every command that
/// prints JSON supports it. `completions` prints a shell script, never JSON.
fn command_supports_cli_jsonl(command: &Commands) -> bool {
    !matches!(command, Commands::Completions { .. })
}

/// Whether `command` only reads: running it again changes nothing on the
/// tracker or on disk, beyond refreshing the local cache. A command not
/// listed here counts as a write, so that a new command is never reported
/// as failed after its write was done.
fn is_read_only(command: &Commands) -> bool {
    use cli::{
        AgentCommands, ArticleCommands, BundleCommands, CacheCommands, ConfigCommands,
        EvalCommands, FieldCommands, IssueCommands, ProjectCommands, TagCommands,
    };
    match command {
        Commands::Issue { action } => matches!(
            action,
            IssueCommands::Create { dry_run: true, .. }
                | IssueCommands::Update { dry_run: true, .. }
                | IssueCommands::Get { .. }
                | IssueCommands::Search { .. }
                | IssueCommands::Inspect { .. }
                | IssueCommands::Attachments { .. }
                | IssueCommands::Comments { .. }
                | IssueCommands::History { .. }
        ),
        Commands::Project { action } => matches!(
            action,
            ProjectCommands::List | ProjectCommands::Get { .. } | ProjectCommands::Fields { .. }
        ),
        Commands::Tags { action } => matches!(action, TagCommands::List),
        Commands::Cache { action } => matches!(
            action,
            CacheCommands::Status | CacheCommands::Show | CacheCommands::Path
        ),
        Commands::Eval { action } => matches!(
            action,
            EvalCommands::Run { .. }
                | EvalCommands::RunAll { .. }
                | EvalCommands::List { .. }
                | EvalCommands::Show { .. }
                | EvalCommands::Status
        ),
        Commands::Config { action } => matches!(
            action,
            ConfigCommands::Get { .. }
                | ConfigCommands::Show
                | ConfigCommands::Keys
                | ConfigCommands::Path
                | ConfigCommands::Test
        ),
        Commands::Article { action } => matches!(
            action,
            ArticleCommands::Get { .. }
                | ArticleCommands::List { .. }
                | ArticleCommands::Search { .. }
                | ArticleCommands::Tree { .. }
                | ArticleCommands::Attachments { .. }
                | ArticleCommands::Comments { .. }
        ),
        Commands::Field { action } => matches!(action, FieldCommands::List),
        Commands::Bundle { action } => matches!(action, BundleCommands::List { .. }),
        Commands::Agent { action } => matches!(action, AgentCommands::Status),
        Commands::Apply { dry_run, .. } => *dry_run,
        // `External` is `track PROJ-1`, the `issue get` shortcut.
        Commands::Doctor { .. }
        | Commands::Workflow { .. }
        | Commands::Context { .. }
        | Commands::Prime { .. }
        | Commands::Guide { .. }
        | Commands::Completions { .. }
        | Commands::External(_) => true,
        Commands::Init { .. } | Commands::Open { .. } => false,
    }
}

fn run(cli: Cli) -> Result<()> {
    if cli.format.is_jsonl() && !command_supports_cli_jsonl(&cli.command) {
        return Err(anyhow!(
            "jsonl output is unsupported for this command: it does not print JSON"
        ));
    }
    output::set_jsonl_mode(cli.format.is_jsonl());
    // One read of the config chain serves the HTTPS trust roots, the default
    // backend and the collapsed config below. Load errors count as defaults
    // until that collapse reports them. Trust roots are fixed when the first
    // client is built, and doctor and init build clients before the collapse.
    let raw_config = Config::load_raw(cli.config.clone());
    tracker_core::tls::set_system_certs(
        raw_config
            .as_ref()
            .is_ok_and(|config| config.system_certs == Some(true)),
    );

    // `--select` projects whatever JSON a command prints. Text has no
    // fields to select, and a flag with no usable paths is not a successful
    // no-op projection. A selection that matches nothing fails a read, but
    // not a write that is already done.
    if let Some(select) = cli.select.as_deref() {
        if cli.format.output() == cli::OutputFormat::Text {
            return Err(anyhow!(
                "--select needs JSON output: add -o json or -o jsonl"
            ));
        }
        let paths = parse_select_paths(select);
        if paths.is_empty() {
            return Err(anyhow!("--select requires at least one field path"));
        }
        output::set_select_paths(paths, is_read_only(&cli.command));
    }

    // Handle completions command - no API needed
    if let Commands::Completions { shell } = &cli.command {
        Cli::generate_completions(*shell);
        return Ok(());
    }

    // Handle eval command - no API needed (uses mock system)
    if let Commands::Eval { action } = &cli.command {
        return commands::eval::handle_eval(action, cli.format.output());
    }

    // Agent setup and the embedded guide need no config or API either.
    if let Commands::Agent { action } = &cli.command {
        return commands::agent::handle_agent(action, cli.format.output());
    }
    if let Commands::Guide { topic } = &cli.command {
        return commands::agent::handle_guide(topic.as_deref(), cli.format.output());
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
            cli.format.output(),
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
            | ConfigCommands::Get { .. }
            | ConfigCommands::Untrust => {
                return commands::config::handle_config_local(
                    action,
                    cli.config.as_deref(),
                    cli.format.output(),
                );
            }
            ConfigCommands::Backend { backend } => {
                return commands::config::handle_config_backend(*backend, cli.format.output());
            }
            ConfigCommands::Project { .. } | ConfigCommands::Test | ConfigCommands::Trust => {
                // These commands need the loaded config (Trust) or an API connection
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
        commands::open::shortcut_issue_id(args)?;
    }

    // Effective backend: -b / TRACKER_BACKEND, else `backend` from the config
    // files (project over global, or only the --config file), else YouTrack
    let effective_backend = cli.backend.unwrap_or_else(|| {
        raw_config
            .as_ref()
            .map(Config::get_backend)
            .unwrap_or_default()
    });

    let mut config = raw_config?.for_backend(effective_backend);
    config.merge_with_cli(cli.url.clone(), cli.token.clone());

    if let Commands::Config {
        action: cli::ConfigCommands::Trust,
    } = &cli.command
    {
        return commands::config::handle_config_trust(
            cli.config.as_deref(),
            effective_backend,
            cli.url.as_deref(),
            cli.token.as_deref(),
            &config,
            cli.format.output(),
        );
    }

    cache::set_active_backend(
        &effective_backend.to_string(),
        cache_scope(effective_backend, &config),
    );

    // Prime is offline too, and in hook mode it only speaks for a directory
    // that is set up for track.
    if let Commands::Prime {
        project,
        hook,
        max_values,
    } = &cli.command
    {
        let mock = tracker_mock::get_mock_dir().is_some();
        if !mock {
            let configured_here = cli.config.is_some() || config::is_project_context();
            let valid = config.validate(effective_backend);
            if hook.is_some() && !(configured_here && valid.is_ok()) {
                commands::prime::print_empty_hook_output();
                return Ok(());
            }
            valid.map_err(|err| errors::coded(errors::CliCode::NotConfigured, err))?;
        }
        return commands::prime::handle_prime(&commands::prime::PrimeOptions {
            project: project.as_deref(),
            max_values: *max_values,
            format: cli.format.output(),
            backend: effective_backend,
            url: config.url.as_deref(),
            default_project: config.default_project.as_deref(),
            explicit_config: cli.config.as_deref(),
            hook: *hook,
        });
    }

    // Workflow commands are offline: after config load + backend selection,
    // before credential validation and client construction.
    if let Commands::Workflow { action } = &cli.command {
        return commands::workflow::handle_workflow(
            action,
            cli.format.output(),
            cli.config.as_deref(),
            effective_backend,
        );
    }

    // Check if mock mode is enabled (before config validation, since mock mode
    // doesn't need real backend credentials)
    if let Some(mock_dir) = tracker_mock::get_mock_dir() {
        let client = MockClient::new(&mock_dir)
            .map_err(|e| anyhow::anyhow!("Failed to initialize mock client: {}", e))?;
        return run_with_client(&client, &client, &cli, &config, effective_backend);
    }

    config
        .validate(effective_backend)
        .map_err(|err| errors::coded(errors::CliCode::NotConfigured, err))?;
    token_guard::ensure_token_destination_allowed(
        cli.config.as_deref(),
        effective_backend,
        cli.url.as_deref(),
        cli.token.as_deref(),
        &config,
    )
    .map_err(|err| errors::coded(errors::CliCode::TokenRefused, err))?;

    // Create the appropriate backend client
    let client = build_client(effective_backend, &config)?;
    run_with_client(
        client.issue_tracker(),
        client.knowledge_base(),
        &cli,
        &config,
        effective_backend,
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
                .unwrap_or(config::DEFAULT_LINEAR_API_URL);
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
    backend: Backend,
) -> Result<()> {
    let lookup = lookup::Lookup::new(issue_client, !cli.no_cache);
    match &cli.command {
        Commands::Issue { action } => commands::issue::handle_issue(
            &lookup,
            action,
            cli.format.output(),
            config.default_project.as_deref(),
            cli.verbose,
            config.link_mappings_for(backend),
            cli.config.as_deref(),
        ),
        Commands::Project { action } => {
            commands::project::handle_project(&lookup, action, cli.format.output())
        }
        Commands::Tags { action } => {
            commands::tags::handle_tags(&lookup, action, cli.format.output())
        }
        Commands::Cache { action } => commands::cache::handle_cache(
            issue_client,
            Some(kb_client),
            action,
            cli.format.output(),
            backend,
            config,
        ),
        Commands::Config { action } => commands::config::handle_config(
            issue_client,
            action,
            cli.format.output(),
            config,
            backend,
        ),
        Commands::Article { action } => {
            commands::article::handle_article(issue_client, kb_client, action, cli.format.output())
        }
        Commands::Field { action } => {
            commands::field::handle_field(&lookup, action, cli.format.output())
        }
        Commands::Bundle { action } => {
            commands::bundle::handle_bundle(&lookup, action, cli.format.output())
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

            commands::context::handle_context(
                issue_client,
                Some(kb_client),
                project.as_deref(),
                *refresh,
                *include_issues,
                *issue_limit,
                cli.format.output(),
                backend,
                config.url.as_deref(),
                config.default_project.as_deref(),
                cli.config.as_deref(),
            )
        }
        Commands::Apply {
            plan,
            dry_run,
            validation,
            resume,
            allow_delete,
        } => commands::apply::handle_apply(
            &lookup,
            commands::apply::ApplyOptions {
                plan_path: plan,
                dry_run: *dry_run,
                validation: validation.requested(),
                resume_path: resume.as_deref(),
                allow_delete: *allow_delete,
                format: cli.format.output(),
                default_project: config.default_project.as_deref(),
            },
        ),
        Commands::Completions { .. } => {
            unreachable!("Completions command should be handled before API validation")
        }
        Commands::Init { .. } => {
            unreachable!("Init command should be handled before API validation")
        }
        Commands::Agent { .. } | Commands::Guide { .. } => {
            unreachable!("Agent and guide commands should be handled before API validation")
        }
        Commands::Open { id } => commands::open::handle_open(
            issue_client,
            id.as_deref(),
            config,
            backend,
            cli.format.output(),
        ),
        Commands::External(args) => {
            commands::open::handle_issue_shortcut(issue_client, args, cli.format.output())
        }
        Commands::Eval { .. } => {
            unreachable!("Eval command should be handled before API validation")
        }
        Commands::Doctor { .. } => {
            unreachable!("Doctor command should be handled before API validation")
        }
        Commands::Workflow { .. } => {
            unreachable!("Workflow command should be handled before API validation")
        }
        Commands::Prime { .. } => {
            unreachable!("Prime command should be handled before API validation")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn read_only(args: &[&str]) -> bool {
        let cli = Cli::try_parse_from(std::iter::once("track").chain(args.iter().copied()))
            .unwrap_or_else(|err| panic!("{args:?}: {err}"));
        is_read_only(&cli.command)
    }

    #[test]
    fn env_values_name_a_variant_or_count_as_unset() {
        assert_eq!(value_enum_from(Some(" JSON ")), Some(CliFormat::Json));
        assert_eq!(value_enum_from(Some("agent")), Some(CliFormat::Agent));
        assert_eq!(
            value_enum_from(Some("Json")),
            Some(errors::ErrorFormat::Json)
        );
        for unset in [None, Some(""), Some("  "), Some("yaml")] {
            assert_eq!(value_enum_from::<CliFormat>(unset), None, "{unset:?}");
            assert_eq!(
                value_enum_from::<errors::ErrorFormat>(unset),
                None,
                "{unset:?}"
            );
        }
    }

    #[test]
    fn writes_are_told_apart_from_reads() {
        for args in [
            &["issue", "get", "X-1"][..],
            &["issue", "search", "q"],
            &["X-1"],
            &["article", "list"],
            &["apply", "plan.json", "--dry-run"],
            &["issue", "update", "X-1", "--summary", "x", "--dry-run"],
            &["context"],
            &["config", "show"],
        ] {
            assert!(read_only(args), "{args:?} only reads");
        }
        for args in [
            &["issue", "create", "-s", "x"][..],
            &["issue", "update", "X-1", "--summary", "x"],
            &["issue", "delete", "X-1"],
            &["issue", "comment", "X-1", "-m", "hi"],
            &["issue", "complete", "X-1"],
            &["article", "create", "-p", "KB", "-s", "x"],
            &["apply", "plan.json"],
            &["cache", "refresh"],
            &["config", "set", "url", "x"],
        ] {
            assert!(!read_only(args), "{args:?} writes");
        }
    }
}
