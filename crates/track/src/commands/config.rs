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
        ConfigCommands::Set { key, value } => {
            let config_key = parse_config_key(key)?;

            let config_path = config::local_track_config_path()?;
            let mut cfg = Config::load_local_track_toml()?.unwrap_or_default();
            config_key.set_value(&mut cfg, value)?;
            cfg.save(&config_path)?;

            match format {
                cli::OutputFormat::Json => {
                    output_json(&serde_json::json!({
                        "success": true,
                        "key": config_key.as_str(),
                        "value": value
                    }))?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!(
                        "Set {} = {}",
                        config_key.as_str().cyan().bold(),
                        value.green()
                    );
                }
            }
            Ok(())
        }
        ConfigCommands::Get { key } => {
            let config_key = parse_config_key(key)?;
            let cfg = Config::load_local_track_toml()?.unwrap_or_default();
            let value = config_key.get_value(&cfg);

            match format {
                cli::OutputFormat::Json => {
                    let is_set = value.is_some();
                    let output = serde_json::json!({
                        "key": config_key.as_str(),
                        "value": value,
                        "is_set": is_set
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    if let Some(v) = &value {
                        let display_value = if config_key.is_secret() {
                            "(set - hidden)".to_string()
                        } else {
                            v.to_string()
                        };
                        println!("{} = {}", config_key.as_str().cyan(), display_value.green());
                    } else {
                        println!("{} is not set", config_key.as_str().cyan());
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
                        let backend_name = cfg.backend.unwrap_or_default().to_string();
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
                        output_json(&serde_json::json!({
                            "success": true,
                            "message": "Configuration cleared"
                        }))?;
                    }
                    cli::OutputFormat::Text => {
                        use colored::Colorize;
                        println!("{}", "Default project and backend cleared.".green());
                    }
                }
            } else {
                match format {
                    cli::OutputFormat::Json => {
                        output_json(&serde_json::json!({
                            "success": true,
                            "message": "No configuration to clear"
                        }))?;
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
