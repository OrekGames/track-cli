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

#[derive(Debug, Clone)]
enum InitProject {
    Default { short_name: String },
    GitHub { owner: String, repo: String },
    GitLab { id: String, display_name: String },
}

impl InitProject {
    fn display_name(&self) -> String {
        match self {
            InitProject::Default { short_name, .. } => short_name.clone(),
            InitProject::GitHub { owner, repo } => format!("{owner}/{repo}"),
            InitProject::GitLab { display_name, .. } => display_name.clone(),
        }
    }

    fn default_project(&self) -> Option<String> {
        match self {
            InitProject::Default { short_name, .. } => Some(short_name.clone()),
            InitProject::GitHub { .. } | InitProject::GitLab { .. } => None,
        }
    }
}

fn parse_github_project(project: &str) -> Result<(String, String)> {
    let (owner, repo) = project.split_once('/').ok_or_else(|| {
        anyhow::anyhow!(
            "GitHub requires --project in owner/repo format, got '{}'",
            project
        )
    })?;

    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return Err(anyhow::anyhow!(
            "GitHub requires --project in owner/repo format, got '{}'",
            project
        ));
    }

    Ok((owner.to_string(), repo.to_string()))
}

fn install_agent_skills(format: cli::OutputFormat) -> Result<()> {
    use colored::Colorize;

    // Prefer robust fallback chain for CI/container environments where
    // platform home discovery APIs may fail (notably on Windows runners).
    let home = directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from))
        .or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            Some(std::path::PathBuf::from(format!(
                "{}{}",
                drive.to_string_lossy(),
                path.to_string_lossy()
            )))
        })
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
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

#[allow(clippy::too_many_arguments)]
pub fn handle_init(
    url: Option<&str>,
    token: Option<&str>,
    project: Option<&str>,
    email: Option<&str>,
    format: cli::OutputFormat,
    backend: Backend,
    skills: bool,
    global: bool,
) -> Result<()> {
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

    let config_path = if global {
        config::global_config_path_ensure()?
    } else {
        config::local_track_config_path()?
    };

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

    let validated_project: Option<InitProject> = match backend {
        Backend::GitHub => {
            let proj = project
                .ok_or_else(|| anyhow::anyhow!("GitHub init requires --project owner/repo"))?;
            let (owner, repo) = parse_github_project(proj)?;
            let client = GitHubClient::with_base_url(url, &owner, &repo, token);
            client.get_repo(&owner, &repo).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to validate GitHub repository '{}': {}\nCheck your API URL, token, owner, and repo.",
                    proj,
                    e
                )
            })?;
            Some(InitProject::GitHub { owner, repo })
        }
        Backend::GitLab => {
            let proj = project.ok_or_else(|| {
                anyhow::anyhow!("GitLab init requires --project <PROJECT_ID_OR_PATH>")
            })?;
            let client = GitLabClient::new(url, token, None);
            let gitlab_project = client.get_project(proj).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to validate GitLab project '{}': {}\nCheck your URL, token, and project.",
                    proj,
                    e
                )
            })?;
            let display_name = gitlab_project
                .path_with_namespace
                .clone()
                .or(gitlab_project.name_with_namespace.clone())
                .unwrap_or_else(|| gitlab_project.name.clone());
            Some(InitProject::GitLab {
                id: gitlab_project.id.to_string(),
                display_name,
            })
        }
        Backend::YouTrack | Backend::Jira => {
            let Some(proj) = project else {
                return create_config_and_finish(
                    url,
                    token,
                    effective_email,
                    format,
                    backend,
                    skills,
                    global,
                    config_path,
                    None,
                );
            };

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
                Backend::GitHub | Backend::GitLab => unreachable!("handled above"),
            };

            let matched = projects
                .iter()
                .find(|p| {
                    p.short_name.eq_ignore_ascii_case(proj)
                        || p.id == proj
                        || p.name.eq_ignore_ascii_case(proj)
                })
                .ok_or_else(|| anyhow::anyhow!("Project '{}' not found on server", proj))?;

            Some(InitProject::Default {
                short_name: matched.short_name.clone(),
            })
        }
    };

    create_config_and_finish(
        url,
        token,
        effective_email,
        format,
        backend,
        skills,
        global,
        config_path,
        validated_project,
    )
}

#[allow(clippy::too_many_arguments)]
fn create_config_and_finish(
    url: &str,
    token: &str,
    effective_email: Option<String>,
    format: cli::OutputFormat,
    backend: Backend,
    skills: bool,
    global: bool,
    config_path: std::path::PathBuf,
    validated_project: Option<InitProject>,
) -> Result<()> {
    use colored::Colorize;

    // Create config with backend and optional default project
    let mut config = Config {
        backend: Some(backend),
        url: Some(url.to_string()),
        token: Some(token.to_string()),
        email: effective_email,
        default_project: validated_project
            .as_ref()
            .and_then(InitProject::default_project),
        youtrack: Default::default(),
        jira: Default::default(),
        github: Default::default(),
        gitlab: Default::default(),
    };

    match &validated_project {
        Some(InitProject::GitHub { owner, repo }) => {
            config.url = None;
            config.token = None;
            config.github.owner = Some(owner.clone());
            config.github.repo = Some(repo.clone());
            config.github.token = Some(token.to_string());
            config.github.api_url = Some(url.to_string());
        }
        Some(InitProject::GitLab { id, .. }) => {
            config.url = None;
            config.token = None;
            config.gitlab.url = Some(url.to_string());
            config.gitlab.token = Some(token.to_string());
            config.gitlab.project_id = Some(id.clone());
        }
        Some(InitProject::Default { .. }) => {}
        None => {}
    }

    config.save(&config_path)?;

    // Write agent guide to the same directory as the config (skip for global init)
    let guide_path = if !global {
        let path = config_path
            .parent()
            .map(|p| p.join("AGENT_GUIDE.md"))
            .unwrap_or_else(|| std::path::PathBuf::from("AGENT_GUIDE.md"));
        std::fs::write(&path, AGENT_GUIDE)?;
        Some(path)
    } else {
        None
    };

    // If --skills was also passed, install skill files too
    if skills {
        install_agent_skills(format)?;
    }

    let level = if global { "global" } else { "project" };
    match format {
        cli::OutputFormat::Json => {
            let mut result = serde_json::json!({
                "success": true,
                "level": level,
                "backend": backend.to_string(),
                "config_path": config_path.display().to_string(),
            });
            if let Some(guide) = &guide_path {
                result["guide_path"] = serde_json::json!(guide.display().to_string());
            }
            if let Some(project) = &validated_project {
                result["project"] = serde_json::json!(project.display_name());
            }
            if let Some(default_project) = config.default_project.as_deref() {
                result["default_project"] = serde_json::json!(default_project);
            }
            output_json(&result)?;
        }
        cli::OutputFormat::Text => {
            let tag = if global {
                "[global]".yellow().to_string()
            } else {
                "[project]".cyan().to_string()
            };
            println!(
                "{} {} {}",
                tag,
                "Created config:".green(),
                config_path.display()
            );
            if let Some(guide) = &guide_path {
                println!(
                    "{} {} {}",
                    tag,
                    "Created agent guide:".green(),
                    guide.display()
                );
            }
            println!(
                "  {}: {}",
                "Backend".dimmed(),
                backend.to_string().cyan().bold()
            );
            if let Some(project) = &validated_project {
                println!(
                    "  {}: {}",
                    "Project".dimmed(),
                    project.display_name().cyan().bold()
                );
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
