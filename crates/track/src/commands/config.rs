use anyhow::Result;
use clap::ValueEnum;
use tracker_core::IssueTracker;

use crate::{
    cli::{self, Backend},
    config::{self, Config},
    output::output_json,
};

#[derive(Clone, Copy)]
enum ConfigKey {
    Backend,
    Url,
    Token,
    Email,
    DefaultProject,
    YouTrackUrl,
    YouTrackToken,
    JiraUrl,
    JiraEmail,
    JiraToken,
    GitHubToken,
    GitHubOwner,
    GitHubRepo,
    GitHubApiUrl,
    GitLabToken,
    GitLabUrl,
    GitLabProjectId,
    GitLabNamespace,
}

impl ConfigKey {
    const ALL: [Self; 18] = [
        Self::Backend,
        Self::Url,
        Self::Token,
        Self::Email,
        Self::DefaultProject,
        Self::YouTrackUrl,
        Self::YouTrackToken,
        Self::JiraUrl,
        Self::JiraEmail,
        Self::JiraToken,
        Self::GitHubToken,
        Self::GitHubOwner,
        Self::GitHubRepo,
        Self::GitHubApiUrl,
        Self::GitLabToken,
        Self::GitLabUrl,
        Self::GitLabProjectId,
        Self::GitLabNamespace,
    ];

    fn parse(key: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|candidate| candidate.as_str() == key)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Backend => "backend",
            Self::Url => "url",
            Self::Token => "token",
            Self::Email => "email",
            Self::DefaultProject => "default_project",
            Self::YouTrackUrl => "youtrack.url",
            Self::YouTrackToken => "youtrack.token",
            Self::JiraUrl => "jira.url",
            Self::JiraEmail => "jira.email",
            Self::JiraToken => "jira.token",
            Self::GitHubToken => "github.token",
            Self::GitHubOwner => "github.owner",
            Self::GitHubRepo => "github.repo",
            Self::GitHubApiUrl => "github.api_url",
            Self::GitLabToken => "gitlab.token",
            Self::GitLabUrl => "gitlab.url",
            Self::GitLabProjectId => "gitlab.project_id",
            Self::GitLabNamespace => "gitlab.namespace",
        }
    }

    fn value_type(self) -> &'static str {
        match self {
            Self::Backend => "youtrack | jira | github | gitlab",
            _ => "string",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Backend => "Default backend to use",
            Self::Url => "Tracker instance URL",
            Self::Token => "API token (YouTrack permanent token or Jira API token)",
            Self::Email => "Email for authentication (required for Jira)",
            Self::DefaultProject => "Default project shortName (e.g., \"PROJ\")",
            Self::YouTrackUrl => "YouTrack-specific URL (overrides 'url' when backend=youtrack)",
            Self::YouTrackToken => "YouTrack-specific token",
            Self::JiraUrl => "Jira-specific URL (overrides 'url' when backend=jira)",
            Self::JiraEmail => "Jira-specific email",
            Self::JiraToken => "Jira-specific token",
            Self::GitHubToken => "GitHub personal access token",
            Self::GitHubOwner => "GitHub repository owner (user or organization)",
            Self::GitHubRepo => "GitHub repository name",
            Self::GitHubApiUrl => "GitHub API URL (defaults to https://api.github.com)",
            Self::GitLabToken => "GitLab personal access token",
            Self::GitLabUrl => "GitLab instance URL (e.g., https://gitlab.com)",
            Self::GitLabProjectId => "GitLab numeric project ID",
            Self::GitLabNamespace => "GitLab namespace/group path",
        }
    }

    fn is_secret(self) -> bool {
        matches!(
            self,
            Self::Token
                | Self::YouTrackToken
                | Self::JiraToken
                | Self::GitHubToken
                | Self::GitLabToken
        )
    }

    fn set_value(self, cfg: &mut Config, value: &str) -> Result<()> {
        match self {
            Self::Backend => {
                let backend = Backend::from_str(value, true).map_err(|_| {
                    anyhow::anyhow!(
                        "Invalid backend '{}'. Valid: youtrack (yt), jira (j), github (gh), gitlab (gl)",
                        value
                    )
                })?;
                cfg.backend = Some(backend);
            }
            Self::Url => cfg.url = Some(value.to_string()),
            Self::Token => cfg.token = Some(value.to_string()),
            Self::Email => cfg.email = Some(value.to_string()),
            Self::DefaultProject => cfg.default_project = Some(value.to_string()),
            Self::YouTrackUrl => cfg.youtrack.url = Some(value.to_string()),
            Self::YouTrackToken => cfg.youtrack.token = Some(value.to_string()),
            Self::JiraUrl => cfg.jira.url = Some(value.to_string()),
            Self::JiraEmail => cfg.jira.email = Some(value.to_string()),
            Self::JiraToken => cfg.jira.token = Some(value.to_string()),
            Self::GitHubToken => cfg.github.token = Some(value.to_string()),
            Self::GitHubOwner => cfg.github.owner = Some(value.to_string()),
            Self::GitHubRepo => cfg.github.repo = Some(value.to_string()),
            Self::GitHubApiUrl => cfg.github.api_url = Some(value.to_string()),
            Self::GitLabToken => cfg.gitlab.token = Some(value.to_string()),
            Self::GitLabUrl => cfg.gitlab.url = Some(value.to_string()),
            Self::GitLabProjectId => cfg.gitlab.project_id = Some(value.to_string()),
            Self::GitLabNamespace => cfg.gitlab.namespace = Some(value.to_string()),
        }
        Ok(())
    }

    fn get_value(self, cfg: &Config) -> Option<String> {
        match self {
            Self::Backend => cfg.backend.map(|b| b.to_string()),
            Self::Url => cfg.url.clone(),
            Self::Token => cfg.token.clone(),
            Self::Email => cfg.email.clone(),
            Self::DefaultProject => cfg.default_project.clone(),
            Self::YouTrackUrl => cfg.youtrack.url.clone(),
            Self::YouTrackToken => cfg.youtrack.token.clone(),
            Self::JiraUrl => cfg.jira.url.clone(),
            Self::JiraEmail => cfg.jira.email.clone(),
            Self::JiraToken => cfg.jira.token.clone(),
            Self::GitHubToken => cfg.github.token.clone(),
            Self::GitHubOwner => cfg.github.owner.clone(),
            Self::GitHubRepo => cfg.github.repo.clone(),
            Self::GitHubApiUrl => cfg.github.api_url.clone(),
            Self::GitLabToken => cfg.gitlab.token.clone(),
            Self::GitLabUrl => cfg.gitlab.url.clone(),
            Self::GitLabProjectId => cfg.gitlab.project_id.clone(),
            Self::GitLabNamespace => cfg.gitlab.namespace.clone(),
        }
    }
}

fn parse_config_key(key: &str) -> Result<ConfigKey> {
    ConfigKey::parse(key).ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid configuration key: '{}'\nRun 'track config keys' to see valid keys.",
            key
        )
    })
}

/// Handle config backend command
pub fn handle_config_backend(backend: Backend, format: cli::OutputFormat) -> Result<()> {
    Config::update_backend(backend)?;
    let backend_name = backend.to_string();

    match format {
        cli::OutputFormat::Json => {
            output_json(&serde_json::json!({
                "success": true,
                "backend": backend_name
            }))?;
        }
        cli::OutputFormat::Text => {
            use colored::Colorize;
            println!("Default backend set to: {}", backend_name.cyan().bold());
        }
    }
    Ok(())
}

/// Handle config commands that don't need API connection
pub fn handle_config_local(action: &cli::ConfigCommands, format: cli::OutputFormat) -> Result<()> {
    use cli::ConfigCommands;

    match action {
        ConfigCommands::Keys => {
            match format {
                cli::OutputFormat::Json => {
                    let keys: Vec<serde_json::Value> = ConfigKey::ALL
                        .iter()
                        .map(|key| {
                            serde_json::json!({
                                "key": key.as_str(),
                                "type": key.value_type(),
                                "description": key.description()
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&keys)?);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{}:", "Available Configuration Keys".white().bold());
                    println!();
                    for key in ConfigKey::ALL {
                        println!(
                            "  {} ({})",
                            key.as_str().cyan().bold(),
                            key.value_type().dimmed()
                        );
                        println!("    {}", key.description());
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
        ConfigCommands::Set { key, value, global } => {
            let config_key = parse_config_key(key)?;

            let (config_path, mut cfg) = if *global {
                let path = config::global_config_path_ensure()?;
                let cfg = Config::load_global_track_toml()?.unwrap_or_default();
                (path, cfg)
            } else {
                let path = config::local_track_config_path()?;
                let cfg = Config::load_local_track_toml()?.unwrap_or_default();
                (path, cfg)
            };
            config_key.set_value(&mut cfg, value)?;
            cfg.save(&config_path)?;

            let level = if *global { "global" } else { "project" };
            match format {
                cli::OutputFormat::Json => {
                    output_json(&serde_json::json!({
                        "success": true,
                        "key": config_key.as_str(),
                        "value": value,
                        "level": level
                    }))?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    let tag = if *global {
                        "[global]".yellow().to_string()
                    } else {
                        "[project]".cyan().to_string()
                    };
                    println!(
                        "{} Set {} = {}",
                        tag,
                        config_key.as_str().cyan().bold(),
                        value.green()
                    );
                }
            }
            Ok(())
        }
        ConfigCommands::Get { key } => {
            let config_key = parse_config_key(key)?;

            // Check both levels to determine source
            let global_cfg = Config::load_global_track_toml()?.unwrap_or_default();
            let project_cfg = Config::load_local_track_toml()?.unwrap_or_default();

            let global_val = config_key.get_value(&global_cfg);
            let project_val = config_key.get_value(&project_cfg);

            // Effective value: project overrides global
            let (effective_val, source) = match (&project_val, &global_val) {
                (Some(_), _) => (&project_val, "project"),
                (None, Some(_)) => (&global_val, "global"),
                (None, None) => (&None, ""),
            };

            match format {
                cli::OutputFormat::Json => {
                    output_json(&serde_json::json!({
                        "key": config_key.as_str(),
                        "value": effective_val,
                        "source": if source.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(source.to_string()) },
                        "is_set": effective_val.is_some(),
                        "global_value": global_val,
                        "project_value": project_val,
                    }))?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    if let Some(v) = effective_val {
                        let display_value = if config_key.is_secret() {
                            "(set - hidden)".to_string()
                        } else {
                            v.to_string()
                        };
                        let tag = match source {
                            "project" => "[project]".cyan().to_string(),
                            "global" => "[global]".yellow().to_string(),
                            _ => String::new(),
                        };
                        println!(
                            "{} {} = {}",
                            tag,
                            config_key.as_str().cyan(),
                            display_value.green()
                        );
                    } else {
                        println!("{} is not set", config_key.as_str().cyan());
                    }
                }
            }
            Ok(())
        }
        ConfigCommands::Show => {
            use colored::Colorize;

            let global_cfg = Config::load_global_track_toml()?.unwrap_or_default();
            let project_cfg = Config::load_local_track_toml()?;
            let has_global = config::global_config_path()
                .map(|p| p.exists())
                .unwrap_or(false);
            let has_project = project_cfg.is_some();
            let project_cfg = project_cfg.unwrap_or_default();

            if !has_global && !has_project {
                match format {
                    cli::OutputFormat::Json => {
                        println!("{{}}");
                    }
                    cli::OutputFormat::Text => {
                        println!("No configuration found.");
                        println!(
                            "Run '{}' or '{}' to create one.",
                            "track init --url <URL> --token <TOKEN>".cyan(),
                            "track init --global --url <URL> --token <TOKEN>".cyan()
                        );
                    }
                }
                return Ok(());
            }

            match format {
                cli::OutputFormat::Json => {
                    let mut entries = Vec::new();
                    for key in ConfigKey::ALL {
                        let global_val = key.get_value(&global_cfg);
                        let project_val = key.get_value(&project_cfg);
                        let (effective, source) = match (&project_val, &global_val) {
                            (Some(_), _) => (&project_val, "project"),
                            (None, Some(_)) => (&global_val, "global"),
                            (None, None) => continue,
                        };
                        let display_val = if key.is_secret() {
                            Some("(set - hidden)".to_string())
                        } else {
                            effective.clone()
                        };
                        entries.push(serde_json::json!({
                            "key": key.as_str(),
                            "value": display_val,
                            "source": source,
                        }));
                    }
                    output_json(&serde_json::json!({ "config": entries }))?;
                }
                cli::OutputFormat::Text => {
                    // Show file paths
                    println!("{}:", "Configuration".white().bold());
                    if let Some(global_path) = config::global_config_path() {
                        let status = if has_global {
                            "(exists)".green().to_string()
                        } else {
                            "(not found)".dimmed().to_string()
                        };
                        println!(
                            "  {} {} {}",
                            "[global]".yellow(),
                            global_path.display(),
                            status
                        );
                    }
                    let project_path = config::local_track_config_path()?;
                    let status = if has_project {
                        "(exists)".green().to_string()
                    } else {
                        "(not found)".dimmed().to_string()
                    };
                    println!(
                        "  {} {} {}",
                        "[project]".cyan(),
                        project_path.display(),
                        status
                    );
                    println!();

                    // Helper to show a value with source tag
                    let show_value = |label: &str,
                                      global_val: &Option<String>,
                                      project_val: &Option<String>,
                                      is_secret: bool| {
                        let (val, tag) = match (project_val, global_val) {
                            (Some(v), _) => (Some(v.as_str()), "[project]".cyan().to_string()),
                            (None, Some(v)) => (Some(v.as_str()), "[global]".yellow().to_string()),
                            (None, None) => return,
                        };
                        if let Some(v) = val {
                            let display = if is_secret {
                                "(set)".green().to_string()
                            } else {
                                v.cyan().to_string()
                            };
                            println!("  {} {}: {}", tag, label.dimmed(), display);
                        }
                    };

                    // Top-level keys
                    show_value(
                        "backend",
                        &global_cfg.backend.map(|b| b.to_string()),
                        &project_cfg.backend.map(|b| b.to_string()),
                        false,
                    );
                    show_value("url", &global_cfg.url, &project_cfg.url, false);
                    show_value("token", &global_cfg.token, &project_cfg.token, true);
                    show_value("email", &global_cfg.email, &project_cfg.email, false);
                    show_value(
                        "default_project",
                        &global_cfg.default_project,
                        &project_cfg.default_project,
                        false,
                    );

                    // Backend-specific sections
                    let show_backend_section = |name: &str,
                                                pairs: Vec<(
                        &str,
                        &Option<String>,
                        &Option<String>,
                        bool,
                    )>| {
                        let any_set = pairs.iter().any(|(_, g, p, _)| g.is_some() || p.is_some());
                        if !any_set {
                            return;
                        }
                        println!();
                        println!("  {}:", format!("[{}]", name).white().bold());
                        for (label, global_val, project_val, is_secret) in pairs {
                            show_value(&format!("  {}", label), global_val, project_val, is_secret);
                        }
                    };

                    show_backend_section(
                        "youtrack",
                        vec![
                            (
                                "url",
                                &global_cfg.youtrack.url,
                                &project_cfg.youtrack.url,
                                false,
                            ),
                            (
                                "token",
                                &global_cfg.youtrack.token,
                                &project_cfg.youtrack.token,
                                true,
                            ),
                        ],
                    );
                    show_backend_section(
                        "jira",
                        vec![
                            ("url", &global_cfg.jira.url, &project_cfg.jira.url, false),
                            (
                                "email",
                                &global_cfg.jira.email,
                                &project_cfg.jira.email,
                                false,
                            ),
                            (
                                "token",
                                &global_cfg.jira.token,
                                &project_cfg.jira.token,
                                true,
                            ),
                        ],
                    );
                    show_backend_section(
                        "github",
                        vec![
                            (
                                "token",
                                &global_cfg.github.token,
                                &project_cfg.github.token,
                                true,
                            ),
                            (
                                "owner",
                                &global_cfg.github.owner,
                                &project_cfg.github.owner,
                                false,
                            ),
                            (
                                "repo",
                                &global_cfg.github.repo,
                                &project_cfg.github.repo,
                                false,
                            ),
                            (
                                "api_url",
                                &global_cfg.github.api_url,
                                &project_cfg.github.api_url,
                                false,
                            ),
                        ],
                    );
                    show_backend_section(
                        "gitlab",
                        vec![
                            (
                                "token",
                                &global_cfg.gitlab.token,
                                &project_cfg.gitlab.token,
                                true,
                            ),
                            (
                                "url",
                                &global_cfg.gitlab.url,
                                &project_cfg.gitlab.url,
                                false,
                            ),
                            (
                                "project_id",
                                &global_cfg.gitlab.project_id,
                                &project_cfg.gitlab.project_id,
                                false,
                            ),
                            (
                                "namespace",
                                &global_cfg.gitlab.namespace,
                                &project_cfg.gitlab.namespace,
                                false,
                            ),
                        ],
                    );
                }
            }
            Ok(())
        }
        ConfigCommands::Clear { global } => {
            let (config_path, loaded) = if *global {
                let path = config::global_config_path_ensure()?;
                let cfg = Config::load_global_track_toml()?;
                (path, cfg)
            } else {
                let path = config::local_track_config_path()?;
                let cfg = Config::load_local_track_toml()?;
                (path, cfg)
            };
            let level = if *global { "global" } else { "project" };

            if let Some(mut cfg) = loaded {
                cfg.default_project = None;
                cfg.backend = None;
                cfg.save(&config_path)?;
                match format {
                    cli::OutputFormat::Json => {
                        output_json(&serde_json::json!({
                            "success": true,
                            "level": level,
                            "message": "Configuration cleared"
                        }))?;
                    }
                    cli::OutputFormat::Text => {
                        use colored::Colorize;
                        let tag = if *global {
                            "[global]".yellow().to_string()
                        } else {
                            "[project]".cyan().to_string()
                        };
                        println!("{} {}", tag, "Default project and backend cleared.".green());
                    }
                }
            } else {
                match format {
                    cli::OutputFormat::Json => {
                        output_json(&serde_json::json!({
                            "success": true,
                            "level": level,
                            "message": "No configuration to clear"
                        }))?;
                    }
                    cli::OutputFormat::Text => {
                        use colored::Colorize;
                        let file_name = if *global {
                            "~/.tracker-cli/.track.toml"
                        } else {
                            ".track.toml"
                        };
                        println!("No {} configuration found.", file_name.cyan());
                    }
                }
            }
            Ok(())
        }
        ConfigCommands::Path => {
            match format {
                cli::OutputFormat::Json => {
                    let global = config::global_config_path();
                    let project = config::local_track_config_path().ok();
                    output_json(&serde_json::json!({
                        "global": global.as_ref().map(|p| p.display().to_string()),
                        "global_exists": global.as_ref().map(|p| p.exists()).unwrap_or(false),
                        "project": project.as_ref().map(|p| p.display().to_string()),
                        "project_exists": project.as_ref().map(|p| p.exists()).unwrap_or(false),
                    }))?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    if let Some(global) = config::global_config_path() {
                        let status = if global.exists() {
                            "(exists)".green().to_string()
                        } else {
                            "(not found)".dimmed().to_string()
                        };
                        println!("Global:  {} {}", global.display(), status);
                    }
                    let project = config::local_track_config_path()?;
                    let status = if project.exists() {
                        "(exists)".green().to_string()
                    } else {
                        "(not found)".dimmed().to_string()
                    };
                    println!("Project: {} {}", project.display(), status);
                }
            }
            Ok(())
        }
        ConfigCommands::Project { .. } | ConfigCommands::Test | ConfigCommands::Backend { .. } => {
            // This shouldn't be called - handled elsewhere
            unreachable!("Project/Test/Backend command should be handled elsewhere")
        }
    }
}

pub fn handle_config(
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
                    output_json(&serde_json::json!({
                        "success": true,
                        "default_project": project.short_name
                    }))?;
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
                    output_json(&serde_json::json!({
                        "success": true,
                        "url": url,
                        "projects_count": projects.len()
                    }))?;
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
        | ConfigCommands::Clear { .. }
        | ConfigCommands::Path
        | ConfigCommands::Keys
        | ConfigCommands::Set { .. }
        | ConfigCommands::Get { .. }
        | ConfigCommands::Backend { .. } => {
            unreachable!("Local config commands should be handled before API validation")
        }
    }
}
