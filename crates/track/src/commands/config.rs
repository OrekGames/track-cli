use anyhow::Result;
use clap::ValueEnum;
use tracker_core::IssueTracker;

use crate::{
    cli::{self, Backend},
    config::{self, Config},
    output::output_json,
};

/// Printed in place of a secret value so tokens never reach terminals or logs.
const HIDDEN_SECRET: &str = "(set - hidden)";

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
    LinearToken,
    LinearApiUrl,
    LinearUrl,
    LinearDefaultTeam,
    LinearDefaultProject,
}

impl ConfigKey {
    const ALL: [Self; 23] = [
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
        Self::LinearToken,
        Self::LinearApiUrl,
        Self::LinearUrl,
        Self::LinearDefaultTeam,
        Self::LinearDefaultProject,
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
            Self::LinearToken => "linear.token",
            Self::LinearApiUrl => "linear.api_url",
            Self::LinearUrl => "linear.url",
            Self::LinearDefaultTeam => "linear.default_team",
            Self::LinearDefaultProject => "linear.default_linear_project",
        }
    }

    fn value_type(self) -> &'static str {
        match self {
            Self::Backend => "youtrack | jira | github | gitlab | linear",
            _ => "string",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Backend => "Default backend to use",
            Self::Url => "Tracker instance URL",
            Self::Token => "API token",
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
            Self::LinearToken => "Linear personal API key",
            Self::LinearApiUrl => {
                "Linear GraphQL API URL (defaults to https://api.linear.app/graphql)"
            }
            Self::LinearUrl => "Linear workspace URL used by 'track open'",
            Self::LinearDefaultTeam => "Default Linear team key/name/id",
            Self::LinearDefaultProject => "Default Linear project association for issue create",
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
                | Self::LinearToken
        )
    }

    /// `value` as safe to print: secrets are replaced by [`HIDDEN_SECRET`].
    fn displayed(self, value: &str) -> &str {
        if self.is_secret() {
            HIDDEN_SECRET
        } else {
            value
        }
    }

    fn set_value(self, cfg: &mut Config, value: &str) -> Result<()> {
        match self {
            Self::Backend => {
                let backend = Backend::from_str(value, true).map_err(|_| {
                    anyhow::anyhow!(
                        "Invalid backend '{}'. Valid: youtrack (yt), jira (j), github (gh), gitlab (gl), linear (lin)",
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
            Self::LinearToken => cfg.linear.token = Some(value.to_string()),
            Self::LinearApiUrl => cfg.linear.api_url = Some(value.to_string()),
            Self::LinearUrl => cfg.linear.url = Some(value.to_string()),
            Self::LinearDefaultTeam => cfg.linear.default_team = Some(value.to_string()),
            Self::LinearDefaultProject => {
                cfg.linear.default_linear_project = Some(value.to_string())
            }
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
            Self::LinearToken => cfg.linear.token.clone(),
            Self::LinearApiUrl => cfg.linear.api_url.clone(),
            Self::LinearUrl => cfg.linear.url.clone(),
            Self::LinearDefaultTeam => cfg.linear.default_team.clone(),
            Self::LinearDefaultProject => cfg.linear.default_linear_project.clone(),
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

/// The value of `key` that applies (the project file overrides the global
/// one), with the level it came from.
fn effective_value(
    key: ConfigKey,
    global_cfg: &Config,
    project_cfg: &Config,
) -> Option<(String, &'static str)> {
    key.get_value(project_cfg)
        .map(|value| (value, "project"))
        .or_else(|| key.get_value(global_cfg).map(|value| (value, "global")))
}

/// Colored `[project]` / `[global]` label for a config level.
fn level_tag(level: &str) -> String {
    use colored::Colorize;
    match level {
        "global" => "[global]".yellow().to_string(),
        _ => "[project]".cyan().to_string(),
    }
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
                    output_json(&keys)?;
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

  # Linear-specific settings (used when backend = "linear")
  [linear]
  token = "lin_api_xxx"
  url = "https://linear.app/company"
  default_team = "PROJ"
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
                        "value": config_key.displayed(value),
                        "level": level
                    }))?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!(
                        "{} Set {} = {}",
                        level_tag(level),
                        config_key.as_str().cyan().bold(),
                        config_key.displayed(value).green()
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
            let effective = effective_value(config_key, &global_cfg, &project_cfg);

            match format {
                cli::OutputFormat::Json => {
                    let displayed = |value: Option<String>| {
                        value.map(|value| config_key.displayed(&value).to_string())
                    };
                    output_json(&serde_json::json!({
                        "key": config_key.as_str(),
                        "value": effective.as_ref().map(|(value, _)| config_key.displayed(value)),
                        "source": effective.as_ref().map(|(_, source)| source),
                        "is_set": effective.is_some(),
                        "global_value": displayed(config_key.get_value(&global_cfg)),
                        "project_value": displayed(config_key.get_value(&project_cfg)),
                    }))?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    if let Some((value, source)) = &effective {
                        println!(
                            "{} {} = {}",
                            level_tag(source),
                            config_key.as_str().cyan(),
                            config_key.displayed(value).green()
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
                    let entries: Vec<serde_json::Value> = ConfigKey::ALL
                        .into_iter()
                        .filter_map(|key| {
                            let (value, source) = effective_value(key, &global_cfg, &project_cfg)?;
                            Some(serde_json::json!({
                                "key": key.as_str(),
                                "value": key.displayed(&value),
                                "source": source,
                            }))
                        })
                        .collect();
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

                    // Keys are listed top-level first, then grouped by
                    // backend section, in ConfigKey::ALL order.
                    let mut current_section = None;
                    for key in ConfigKey::ALL {
                        let Some((value, source)) = effective_value(key, &global_cfg, &project_cfg)
                        else {
                            continue;
                        };
                        let label = match key.as_str().split_once('.') {
                            Some((section, label)) => {
                                if current_section != Some(section) {
                                    current_section = Some(section);
                                    println!();
                                    println!("  {}:", format!("[{}]", section).white().bold());
                                }
                                format!("  {}", label)
                            }
                            None => key.as_str().to_string(),
                        };
                        let display = if key.is_secret() {
                            "(set)".green().to_string()
                        } else {
                            value.cyan().to_string()
                        };
                        println!("  {} {}: {}", level_tag(source), label.dimmed(), display);
                    }
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
                        println!(
                            "{} {}",
                            level_tag(level),
                            "Default project and backend cleared.".green()
                        );
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
    backend: Backend,
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
                        || tracker_core::unicode_eq_ignore_case(&p.name, id)
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
            // Test connection by fetching current user info via projects list.
            // Keep the existing probe; only add GitHub-named context when that
            // is the effective backend.
            let projects = if backend == Backend::GitHub {
                client.list_projects().map_err(|error| {
                    let error = super::without_secret(
                        &error.to_string(),
                        config.token.as_deref().unwrap_or(""),
                    );
                    anyhow::anyhow!(
                        "GitHub connection test failed while listing repositories: {error}\nCheck github.token/GITHUB_TOKEN and run 'track -b github doctor' for per-check details."
                    )
                })?
            } else {
                client.list_projects()?
            };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displayed_hides_every_secret_key() {
        for key in ConfigKey::ALL {
            let shown = key.displayed("ghp_secret");
            if key.is_secret() {
                assert_eq!(shown, HIDDEN_SECRET, "{} leaked its value", key.as_str());
            } else {
                assert_eq!(shown, "ghp_secret", "{} must show its value", key.as_str());
            }
        }
        assert!(ConfigKey::GitHubToken.is_secret());
    }

    #[test]
    fn effective_value_prefers_project_over_global() {
        let global_cfg = Config {
            url: Some("https://global.example.com".to_string()),
            email: Some("global@example.com".to_string()),
            ..Default::default()
        };
        let project_cfg = Config {
            url: Some("https://project.example.com".to_string()),
            ..Default::default()
        };

        assert_eq!(
            effective_value(ConfigKey::Url, &global_cfg, &project_cfg),
            Some(("https://project.example.com".to_string(), "project"))
        );
        assert_eq!(
            effective_value(ConfigKey::Email, &global_cfg, &project_cfg),
            Some(("global@example.com".to_string(), "global"))
        );
        assert_eq!(
            effective_value(ConfigKey::Token, &global_cfg, &project_cfg),
            None
        );
    }

    #[test]
    fn config_keys_are_grouped_by_section() {
        // `config show` prints a section header whenever the section changes,
        // so top-level keys must come first and each section's keys must be
        // contiguous.
        let sections: Vec<Option<&str>> = ConfigKey::ALL
            .iter()
            .map(|key| key.as_str().split_once('.').map(|(section, _)| section))
            .collect();
        for (index, section) in sections.iter().enumerate() {
            let first = sections.iter().position(|s| s == section).unwrap();
            assert!(
                sections[first..=index].iter().all(|s| s == section),
                "keys of section {section:?} must be contiguous"
            );
        }
        assert!(sections[0].is_none(), "top-level keys must come first");
    }
}
