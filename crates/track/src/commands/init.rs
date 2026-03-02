use anyhow::Result;

use crate::cli::{self, Backend};
use crate::config::{self, Config};
use crate::output::output_json;
use github_backend::GitHubClient;
use gitlab_backend::GitLabClient;
use jira_backend::JiraClient;
use tracker_core::IssueTracker;
use youtrack_backend::YouTrackClient;

/// Embedded agent guide content - written to project directory during `track init`
const AGENT_GUIDE: &str = include_str!("../../../../docs/agent_guide.md");

/// Embedded agent skill file (shared by all AI coding tools)
const AGENT_SKILL: &str = include_str!("../../../../agent-skills/SKILL.md");

/// AI coding tool directories that support the Agent Skills standard (~/.{tool}/skills/{name}/SKILL.md)
const SKILL_TOOL_DIRS: &[(&str, &str)] = &[
    ("Claude Code", ".claude"),
    ("Copilot", ".copilot"),
    ("Cursor", ".cursor"),
    ("Gemini CLI", ".gemini"),
];

fn install_agent_skills(format: cli::OutputFormat) -> Result<()> {
    use colored::Colorize;

    let home = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    let home = home.home_dir();
    let mut installed = Vec::new();

    for &(tool_name, tool_dir) in SKILL_TOOL_DIRS {
        let skill_dir = home.join(tool_dir).join("skills").join("track");
        let skill_path = skill_dir.join("SKILL.md");

        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(&skill_path, AGENT_SKILL)?;
        installed.push((tool_name, skill_path));
    }

    match format {
        cli::OutputFormat::Json => {
            let files: Vec<serde_json::Value> = installed
                .iter()
                .map(|(tool, path)| {
                    serde_json::json!({
                        "tool": tool,
                        "path": path.display().to_string()
                    })
                })
                .collect();
            output_json(&serde_json::json!({
                "success": true,
                "skills_installed": files
            }))?;
        }
        cli::OutputFormat::Text => {
            for (tool, path) in &installed {
                println!("{} {} skill: {}", "Installed".green(), tool, path.display());
            }
        }
    }

    Ok(())
}

pub fn handle_init(
    url: Option<&str>,
    token: Option<&str>,
    project: Option<&str>,
    email: Option<&str>,
    format: cli::OutputFormat,
    backend: Backend,
    skills: bool,
) -> Result<()> {
    use colored::Colorize;

    // If --skills only (no url/token), just install skill files and return
    if skills && url.is_none() && token.is_none() {
        return install_agent_skills(format);
    }

    // From here on, url and token are required (enforced by clap)
    let url = url.expect("url required when not using --skills alone");
    let token = token.expect("token required when not using --skills alone");

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

    // Create config with backend and optional default project
    let config = Config {
        backend: Some(backend),
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

    // If --skills was also passed, install skill files too
    if skills {
        install_agent_skills(format)?;
    }

    match format {
        cli::OutputFormat::Json => {
            let mut result = serde_json::json!({
                "success": true,
                "backend": backend.to_string(),
                "config_path": config_path.display().to_string(),
                "guide_path": guide_path.display().to_string()
            });
            if let Some((_, name)) = &validated_project {
                result["default_project"] = serde_json::json!(name);
            }
            output_json(&result)?;
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
            println!(
                "  {}: {}",
                "Backend".dimmed(),
                backend.to_string().cyan().bold()
            );
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
