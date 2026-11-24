use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::cli::{self, Backend};
use crate::config::{self, Config};
use crate::output::output_json;
use github_backend::GitHubClient;
use gitlab_backend::GitLabClient;
use jira_backend::JiraClient;
use linear_backend::LinearClient;
use tracker_core::{IssueTracker, TrackerError};
use ureq::http::Uri;
use youtrack_backend::YouTrackClient;

/// Embedded agent guide content - written to project directory during `track init`
const AGENT_GUIDE: &str = include_str!("../../../../docs/agent_guide.md");

const LOCAL_GITIGNORE_ENTRIES: &[&str] = &[".track.toml", ".tracker-cache/"];

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

fn invalid_github_project(project: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "Invalid GitHub project '{project}': expected exactly <owner>/<repo>, for example OrekGames/track-cli. No config was written."
    )
}

fn parse_github_project(project: &str) -> Result<(String, String)> {
    let (owner, repo) = project
        .split_once('/')
        .ok_or_else(|| invalid_github_project(project))?;

    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return Err(invalid_github_project(project));
    }

    Ok((owner.to_string(), repo.to_string()))
}

fn url_has_userinfo(url: &str) -> bool {
    if let Ok(parsed) = url.parse::<Uri>() {
        parsed
            .authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
    } else {
        url.contains('@')
    }
}

/// Omit query and fragment from a displayed server URL.
fn displayed_url(url: &str) -> &str {
    match url.find(['?', '#']) {
        Some(idx) => &url[..idx],
        None => url,
    }
}

/// Strip `?query` / `#fragment` tails so HTTP-client reasons cannot echo them.
fn omit_query_and_fragment_in_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find(['?', '#']) {
        out.push_str(&rest[..idx]);
        rest = &rest[idx + 1..];
        if let Some(end) = rest.find(|c: char| {
            c.is_whitespace() || matches!(c, '\'' | '"' | ')' | ']' | ',' | ';' | ':')
        }) {
            rest = &rest[end..];
        } else {
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

fn sanitize_transport_reason(reason: &str, url: &str, token: &str) -> String {
    let shown = displayed_url(url);
    super::without_secret(
        &omit_query_and_fragment_in_text(&reason.replace(url, shown)),
        token,
    )
}

fn available_projects_hint(projects: &[tracker_core::Project]) -> String {
    const MAX_SHOWN: usize = 10;
    let mut names: Vec<&str> = projects.iter().map(|p| p.short_name.as_str()).collect();
    names.sort_unstable();
    if names.is_empty() {
        return "The server returned no projects.".to_string();
    }
    let shown = &names[..names.len().min(MAX_SHOWN)];
    let mut hint = format!("Available projects: {}", shown.join(", "));
    if names.len() > MAX_SHOWN {
        hint.push_str(&format!(" (+{} more)", names.len() - MAX_SHOWN));
    }
    hint
}

fn invalid_init_url(url: &str) -> anyhow::Error {
    if url_has_userinfo(url) {
        anyhow::anyhow!(
            "Invalid --url: userinfo is not allowed in server URLs. Expected an absolute https:// URL with a host. Plain http:// is allowed only for localhost. GitHub.com uses https://api.github.com. No config was written."
        )
    } else {
        let url = displayed_url(url);
        anyhow::anyhow!(
            "Invalid --url '{url}': expected an absolute https:// URL with a host. Plain http:// is allowed only for localhost. GitHub.com uses https://api.github.com. No config was written."
        )
    }
}

fn validate_init_url(url: &str) -> Result<()> {
    let parsed = url.parse::<Uri>().map_err(|_| invalid_init_url(url))?;

    let scheme = parsed.scheme_str().ok_or_else(|| invalid_init_url(url))?;
    if scheme != "http" && scheme != "https" {
        return Err(invalid_init_url(url));
    }

    let authority = parsed.authority().ok_or_else(|| invalid_init_url(url))?;
    if authority.as_str().contains('@') {
        return Err(invalid_init_url(url));
    }

    let host = parsed.host().ok_or_else(|| invalid_init_url(url))?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);

    if scheme == "http" && !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return Err(invalid_init_url(url));
    }

    Ok(())
}

fn github_init_probe_error(
    url: &str,
    project: &str,
    token: &str,
    err: TrackerError,
) -> anyhow::Error {
    match err {
        TrackerError::Unauthorized => anyhow::anyhow!(
            "GitHub authentication failed while validating '{project}'. Check that the token is valid and can read this repository. No config was written."
        ),
        // GitHubClient reports a 403 with no quota left this way; a plain HTTP
        // 429 keeps the generic copy.
        err if github_backend::is_rate_limit_error(&err) => anyhow::anyhow!(
            "GitHub rate limiting prevented validation of '{project}'. Wait for the limit to reset or use a token with available quota, then retry. No config was written."
        ),
        TrackerError::Api {
            status: 403,
            message,
        } => {
            let mut text = format!(
                "GitHub denied access to '{project}' (403). Confirm the token can access the repository and, if applicable, is authorized for the organization. No config was written."
            );
            let reason = super::without_secret(&message, token);
            if !reason.is_empty() {
                text.push_str(" GitHub said: ");
                text.push_str(&reason);
            }
            anyhow::anyhow!(text)
        }
        TrackerError::Api { status: 404, .. } => anyhow::anyhow!(
            "GitHub could not access repository '{project}' (404). Verify the owner/repo spelling and that the token can see a private repository. No config was written."
        ),
        TrackerError::Http(reason) => {
            let shown_url = displayed_url(url);
            let reason = sanitize_transport_reason(&reason, url, token);
            anyhow::anyhow!(
                "Could not reach the GitHub API at '{shown_url}' while validating '{project}': {reason}. Check the API URL, network, proxy, and TLS setup, then retry. No config was written."
            )
        }
        other => anyhow::anyhow!(
            "Failed to validate GitHub repository '{}': {}\nCheck your API URL, token, owner, and repo.",
            project,
            sanitize_transport_reason(&other.to_string(), url, token)
        ),
    }
}

fn install_agent_skills(format: cli::OutputFormat) -> Result<()> {
    let installed = write_agent_skills()?;

    match format {
        cli::OutputFormat::Json => {
            output_json(&serde_json::json!({
                "success": true,
                "skills_installed": installed_skills_json(&installed)
            }))?;
        }
        cli::OutputFormat::Text => print_installed_skills(&installed),
    }

    Ok(())
}

/// Write the skill file for every supported AI coding tool and return the
/// (tool name, path) of each file written.
fn write_agent_skills() -> Result<Vec<(String, PathBuf)>> {
    super::agent::write_skills(crate::cli::AgentScope::User, &[])
}

fn installed_skills_json(installed: &[(String, PathBuf)]) -> serde_json::Value {
    installed
        .iter()
        .map(|(tool, path)| {
            serde_json::json!({
                "tool": tool,
                "path": path.display().to_string()
            })
        })
        .collect()
}

fn print_installed_skills(installed: &[(String, PathBuf)]) {
    use colored::Colorize;

    for (tool, path) in installed {
        println!("{} {} skill: {}", "Installed".green(), tool, path.display());
    }
}

fn gitignore_contains_entry(content: &str, entry: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == entry
            || (entry.ends_with('/')
                && trimmed.trim_end_matches('/') == entry.trim_end_matches('/'))
    })
}

fn update_gitignore_if_present(config_path: &Path) -> Result<Option<PathBuf>> {
    let Some(config_dir) = config_path.parent() else {
        return Ok(None);
    };
    let gitignore_path = config_dir.join(".gitignore");
    if !gitignore_path.exists() {
        return Ok(None);
    }

    let mut content = std::fs::read_to_string(&gitignore_path)?;
    let missing: Vec<&str> = LOCAL_GITIGNORE_ENTRIES
        .iter()
        .copied()
        .filter(|entry| !gitignore_contains_entry(&content, entry))
        .collect();

    if missing.is_empty() {
        return Ok(None);
    }

    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    for entry in missing {
        content.push_str(entry);
        content.push('\n');
    }
    std::fs::write(&gitignore_path, content)?;

    Ok(Some(gitignore_path))
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

    validate_init_url(url)?;

    let config_path = if global {
        config::global_config_path_ensure()?
    } else {
        config::local_track_config_path()?
    };

    // Check if config already exists
    if config_path.exists() {
        return Err(anyhow::anyhow!(
            "Initialization stopped: config already exists at '{}'. No files were changed. Update it with 'track config set' or remove it only if you intend to replace the configuration.",
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
                            "Jira requires email for authentication.\nUse --email flag or set JIRA_EMAIL environment variable. No config was written."
                        )
                    })?,
            )
        }
        Backend::YouTrack | Backend::GitHub | Backend::GitLab | Backend::Linear => {
            email.map(|e| e.to_string())
        }
    };

    let validated_project: Option<InitProject> = match backend {
        Backend::GitHub => {
            let proj = project.ok_or_else(|| {
                anyhow::anyhow!(
                    "GitHub setup requires --project <owner>/<repo>. Example: --project OrekGames/track-cli. No config was written."
                )
            })?;
            let (owner, repo) = parse_github_project(proj)?;
            let client = GitHubClient::with_base_url(url, &owner, &repo, token);
            client
                .get_repo(&owner, &repo)
                .map_err(|err| github_init_probe_error(url, proj, token, err))?;
            Some(InitProject::GitHub { owner, repo })
        }
        Backend::GitLab => {
            let proj = project.ok_or_else(|| {
                anyhow::anyhow!(
                    "GitLab init requires --project <PROJECT_ID_OR_PATH>. No config was written."
                )
            })?;
            let client = GitLabClient::new(url, token, None);
            let gitlab_project = client.get_project(proj).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to validate GitLab project '{}': {}\nCheck your URL, token, and project. No config was written.",
                    proj,
                    sanitize_transport_reason(&e.to_string(), url, token)
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
        Backend::Linear => {
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

            let api_url = std::env::var("LINEAR_API_URL")
                .unwrap_or_else(|_| "https://api.linear.app/graphql".to_string());
            let client = LinearClient::with_base_url(&api_url, token);
            let projects = client.list_projects().map_err(|e| {
                anyhow::anyhow!(
                    "Failed to connect to Linear or list teams: {}\nCheck your token and LINEAR_API_URL. No config was written.",
                    sanitize_transport_reason(&e.to_string(), &api_url, token)
                )
            })?;

            let matched = projects
                .iter()
                .find(|p| {
                    p.short_name.eq_ignore_ascii_case(proj)
                        || p.id == proj
                        || tracker_core::unicode_eq_ignore_case(&p.name, proj)
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Linear team '{}' not found. {}. No config was written.",
                        proj,
                        available_projects_hint(&projects)
                    )
                })?;

            Some(InitProject::Default {
                short_name: matched.short_name.clone(),
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
                            "Failed to connect to server or list projects: {}\nCheck your URL and token. No config was written.",
                            sanitize_transport_reason(&e.to_string(), url, token)
                        )
                    })?
                }
                Backend::Jira => {
                    let client = JiraClient::new(url, effective_email.as_ref().unwrap(), token);
                    let tracker: &dyn IssueTracker = &client;
                    tracker.list_projects().map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to connect to server or list projects: {}\nCheck your URL, email, and token. No config was written.",
                            sanitize_transport_reason(&e.to_string(), url, token)
                        )
                    })?
                }
                Backend::GitHub | Backend::GitLab | Backend::Linear => {
                    unreachable!("handled above")
                }
            };

            let matched = projects
                .iter()
                .find(|p| {
                    p.short_name.eq_ignore_ascii_case(proj)
                        || p.id == proj
                        || tracker_core::unicode_eq_ignore_case(&p.name, proj)
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Project '{}' not found on server. {}. No config was written.",
                        proj,
                        available_projects_hint(&projects)
                    )
                })?;

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
    config_path: PathBuf,
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
        system_certs: None,
        youtrack: Default::default(),
        jira: Default::default(),
        github: Default::default(),
        gitlab: Default::default(),
        linear: Default::default(),
        workflow_pack: None,
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
        Some(InitProject::Default { short_name }) if backend == Backend::Linear => {
            config.url = None;
            config.token = None;
            config.linear.url = Some(url.to_string());
            config.linear.token = Some(token.to_string());
            config.linear.default_team = Some(short_name.clone());
        }
        Some(InitProject::Default { .. }) => {}
        None if backend == Backend::Linear => {
            config.url = None;
            config.token = None;
            config.linear.url = Some(url.to_string());
            config.linear.token = Some(token.to_string());
        }
        None => {}
    }

    config.save(&config_path)?;

    let gitignore_path = if !global {
        match update_gitignore_if_present(&config_path) {
            Ok(path) => path,
            Err(error) => {
                let gitignore = config_path
                    .parent()
                    .map(|dir| dir.join(".gitignore"))
                    .unwrap_or_else(|| PathBuf::from(".gitignore"));
                return Err(anyhow::anyhow!(
                    "Config was created at '{}', but '{}' could not be updated: {error}\nAdd '.track.toml' and '.tracker-cache/' manually before committing.",
                    config_path.display(),
                    gitignore.display()
                ));
            }
        }
    } else {
        None
    };

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

    // If --skills was also passed, install skill files too. They are reported
    // with the config result so JSON mode prints a single document.
    let installed_skills = if skills {
        Some(write_agent_skills()?)
    } else {
        None
    };

    let level = if global { "global" } else { "project" };
    match format {
        cli::OutputFormat::Json => {
            let mut result = serde_json::json!({
                "success": true,
                "level": level,
                "backend": backend.to_string(),
                "config_path": config_path.display().to_string(),
            });
            if let Some(installed) = &installed_skills {
                result["skills_installed"] = installed_skills_json(installed);
            }
            if let Some(guide) = &guide_path {
                result["guide_path"] = serde_json::json!(guide.display().to_string());
            }
            if let Some(gitignore) = &gitignore_path {
                result["gitignore_path"] = serde_json::json!(gitignore.display().to_string());
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
            if let Some(installed) = &installed_skills {
                print_installed_skills(installed);
            }
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
            if let Some(gitignore) = &gitignore_path {
                println!(
                    "{} {} {}",
                    tag,
                    "Updated .gitignore:".green(),
                    gitignore.display()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displayed_url_strips_query_and_fragment() {
        assert_eq!(
            displayed_url("https://api.example.com"),
            "https://api.example.com"
        );
        assert_eq!(
            displayed_url("https://api.example.com?token=secret"),
            "https://api.example.com"
        );
        assert_eq!(
            displayed_url("https://api.example.com#section"),
            "https://api.example.com"
        );
        assert_eq!(
            displayed_url("https://api.example.com?a=1#frag"),
            "https://api.example.com"
        );
    }

    #[test]
    fn omit_query_and_fragment_strips_from_transport_reasons() {
        assert_eq!(
            omit_query_and_fragment_in_text("GET https://api.example.com failed"),
            "GET https://api.example.com failed"
        );
        assert_eq!(
            omit_query_and_fragment_in_text("GET https://api.example.com?token=abc failed"),
            "GET https://api.example.com failed"
        );
        assert_eq!(
            omit_query_and_fragment_in_text("GET https://api.example.com#frag failed"),
            "GET https://api.example.com failed"
        );
    }

    #[test]
    fn omit_query_and_fragment_handles_multiple_urls_in_text() {
        assert_eq!(
            omit_query_and_fragment_in_text(
                "connect https://a.example.com?x=1 then https://b.example.com#f"
            ),
            "connect https://a.example.com then https://b.example.com"
        );
    }

    #[test]
    fn omit_query_and_fragment_stops_at_delimiters() {
        assert_eq!(
            omit_query_and_fragment_in_text("GET https://api.example.com?a=1: timeout"),
            "GET https://api.example.com: timeout"
        );
        assert_eq!(
            omit_query_and_fragment_in_text("url (https://api.example.com?q) is unreachable"),
            "url (https://api.example.com) is unreachable"
        );
    }

    #[test]
    fn url_has_userinfo_detects_credentials() {
        assert!(url_has_userinfo("https://user:pass@api.example.com"));
        assert!(url_has_userinfo("https://user@api.example.com"));
        assert!(!url_has_userinfo("https://api.example.com"));
    }

    #[test]
    fn sanitize_transport_reason_strips_query_and_redacts_token() {
        let reason = "GET https://youtrack.example.com/api/projects?token=secret123: refused; token secret123 invalid";
        let out = sanitize_transport_reason(reason, "https://youtrack.example.com", "secret123");
        assert!(!out.contains("secret123"), "got: {out}");
        assert!(!out.contains("?token="), "got: {out}");
        assert!(out.contains("https://youtrack.example.com"), "got: {out}");
        assert!(out.contains("<redacted>"), "got: {out}");
    }

    #[test]
    fn available_projects_hint_lists_names_and_caps_at_ten() {
        let projects: Vec<tracker_core::Project> = (0..12)
            .map(|i| tracker_core::Project {
                id: format!("id-{i}"),
                name: format!("Project {i}"),
                short_name: format!("P{i:02}"),
                description: None,
            })
            .collect();
        let hint = available_projects_hint(&projects);
        assert!(hint.starts_with("Available projects: "), "got: {hint}");
        assert!(hint.contains("P00"), "got: {hint}");
        assert!(hint.contains("(+2 more)"), "got: {hint}");
        assert!(!hint.contains("P10"), "got: {hint}");
        assert_eq!(
            available_projects_hint(&[]),
            "The server returned no projects."
        );
    }
}
