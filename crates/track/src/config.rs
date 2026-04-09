use crate::cli::Backend;
use anyhow::{Result, anyhow};
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Main configuration structure supporting multiple backends
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Config {
    /// Default backend to use (youtrack, jira, github, or gitlab)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<Backend>,
    /// Global URL override (applies to any backend)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Global token override (applies to any backend)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Email for authentication (required for Jira)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Default project shortName (e.g., "PROJ")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_project: Option<String>,
    /// YouTrack-specific configuration
    #[serde(default, skip_serializing_if = "BackendConfig::is_empty")]
    pub youtrack: BackendConfig,
    /// Jira-specific configuration
    #[serde(default, skip_serializing_if = "JiraConfig::is_empty")]
    pub jira: JiraConfig,
    /// GitHub-specific configuration
    #[serde(default, skip_serializing_if = "GitHubConfig::is_empty")]
    pub github: GitHubConfig,
    /// GitLab-specific configuration
    #[serde(default, skip_serializing_if = "GitLabConfig::is_empty")]
    pub gitlab: GitLabConfig,
}

/// Backend-specific configuration
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct BackendConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub link_mappings: HashMap<String, String>,
}

impl BackendConfig {
    pub fn is_empty(&self) -> bool {
        self.url.is_none() && self.token.is_none() && self.link_mappings.is_empty()
    }
}

/// Jira-specific configuration
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct JiraConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub link_mappings: HashMap<String, String>,
}

impl JiraConfig {
    pub fn is_empty(&self) -> bool {
        self.url.is_none()
            && self.email.is_none()
            && self.token.is_none()
            && self.link_mappings.is_empty()
    }
}

/// GitHub-specific configuration
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct GitHubConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// API URL (defaults to https://api.github.com)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
}

impl GitHubConfig {
    pub fn is_empty(&self) -> bool {
        self.token.is_none()
            && self.owner.is_none()
            && self.repo.is_none()
            && self.api_url.is_none()
    }
}

/// GitLab-specific configuration
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct GitLabConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub link_mappings: HashMap<String, String>,
}

impl GitLabConfig {
    pub fn is_empty(&self) -> bool {
        self.token.is_none()
            && self.url.is_none()
            && self.project_id.is_none()
            && self.namespace.is_none()
            && self.link_mappings.is_empty()
    }
}

impl Config {
    pub fn load(config_path: Option<PathBuf>, backend: Backend) -> Result<Self> {
        let mut figment = Figment::new().merge(Serialized::defaults(Config::default()));

        let explicit_path = config_path.as_deref();
        if let Some(path) = explicit_path {
            if !path.exists() {
                return Err(anyhow!("Config file not found: {}", path.display()));
            }
        }

        for path in config_paths(explicit_path) {
            if path.exists() {
                figment = figment.merge(Toml::file(path));
            }
        }

        // Support TRACKER_*, YOUTRACK_*, and JIRA_* environment variables
        figment = figment
            .merge(Env::prefixed("TRACKER_"))
            .merge(Env::prefixed("YOUTRACK_").map(|key| {
                // Map YOUTRACK_URL -> youtrack.url for nested config
                match key.as_str() {
                    "url" => "youtrack.url".into(),
                    "token" => "youtrack.token".into(),
                    _ => key.into(),
                }
            }))
            .merge(Env::prefixed("JIRA_").map(|key| {
                // Map JIRA_URL -> jira.url for nested config
                match key.as_str() {
                    "url" => "jira.url".into(),
                    "email" => "jira.email".into(),
                    "token" => "jira.token".into(),
                    _ => key.into(),
                }
            }))
            .merge(Env::prefixed("GITHUB_").map(|key| match key.as_str() {
                "token" => "github.token".into(),
                "owner" => "github.owner".into(),
                "repo" => "github.repo".into(),
                "api_url" => "github.api_url".into(),
                _ => key.into(),
            }))
            .merge(Env::prefixed("GITLAB_").map(|key| match key.as_str() {
                "token" => "gitlab.token".into(),
                "url" => "gitlab.url".into(),
                "project_id" => "gitlab.project_id".into(),
                "namespace" => "gitlab.namespace".into(),
                _ => key.into(),
            }));

        let mut config: Config = figment
            .extract()
            .map_err(|e| anyhow!("Failed to load config: {}", e))?;

        // Merge backend-specific config with global config
        config.apply_backend_config(backend);

        Ok(config)
    }

    /// Apply backend-specific configuration, falling back to global settings
    fn apply_backend_config(&mut self, backend: Backend) {
        match backend {
            Backend::YouTrack => {
                if let Some(u) = self.youtrack.url.take() {
                    self.url = Some(u);
                }
                if let Some(t) = self.youtrack.token.take() {
                    self.token = Some(t);
                }
            }
            Backend::Jira => {
                if let Some(u) = self.jira.url.take() {
                    self.url = Some(u);
                }
                if let Some(e) = self.jira.email.take() {
                    self.email = Some(e);
                }
                if let Some(t) = self.jira.token.take() {
                    self.token = Some(t);
                }
            }
            Backend::GitHub => {
                if let Some(api_url) = self.github.api_url.take() {
                    self.url = Some(api_url);
                } else {
                    // GitHub typically defaults to api.github.com.
                    // If the global generic URL is set to a completely different service (like YouTrack/GitLab),
                    // we should disregard it and use the GitHub default to prevent cross-contamination.
                    let is_github_url = self
                        .url
                        .as_deref()
                        .is_some_and(|u| u.to_lowercase().contains("github"));

                    if !is_github_url {
                        self.url = Some("https://api.github.com".to_string());
                    }
                }
                if let Some(t) = self.github.token.take() {
                    self.token = Some(t);
                }
            }
            Backend::GitLab => {
                if let Some(u) = self.gitlab.url.take() {
                    self.url = Some(u);
                }
                if let Some(t) = self.gitlab.token.take() {
                    self.token = Some(t);
                }
            }
        }
    }

    pub fn merge_with_cli(&mut self, cli_url: Option<String>, cli_token: Option<String>) {
        if let Some(url) = cli_url {
            self.url = Some(url);
        }
        if let Some(token) = cli_token {
            self.token = Some(token);
        }
    }

    pub fn validate(&self, backend: Backend) -> Result<()> {
        let backend_name = match backend {
            Backend::YouTrack => "YouTrack",
            Backend::Jira => "Jira",
            Backend::GitHub => "GitHub",
            Backend::GitLab => "GitLab",
        };

        if self.url.is_none() {
            return Err(anyhow!(
                "{} URL not configured. Set via --url, TRACKER_URL env var, or config file",
                backend_name
            ));
        }
        if self.token.is_none() {
            return Err(anyhow!(
                "{} token not configured. Set via --token, TRACKER_TOKEN env var, or config file",
                backend_name
            ));
        }
        // Jira requires email for Basic Auth
        if backend == Backend::Jira && self.email.is_none() {
            return Err(anyhow!(
                "Jira email not configured. Set via JIRA_EMAIL env var or config file"
            ));
        }
        Ok(())
    }

    /// Save configuration to a TOML file
    pub fn save(&self, path: &Path) -> Result<()> {
        let toml_string = toml::to_string_pretty(self)
            .map_err(|e| anyhow!("Failed to serialize config: {}", e))?;
        fs::write(path, toml_string).map_err(|e| anyhow!("Failed to write config file: {}", e))?;
        Ok(())
    }

    /// Load config from only the local .track.toml file (for updating it)
    pub fn load_local_track_toml() -> Result<Option<Self>> {
        let path = local_track_config_path()?;
        Self::load_from_path(&path)
    }

    /// Load config from only the global ~/.tracker-cli/.track.toml file (for updating it)
    pub fn load_global_track_toml() -> Result<Option<Self>> {
        let path = global_config_path()
            .ok_or_else(|| anyhow!("Could not determine home directory for global config"))?;
        Self::load_from_path(&path)
    }

    /// Load config from a specific path
    fn load_from_path(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path)
            .map_err(|e| anyhow!("Failed to read {}: {}", path.display(), e))?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse {}: {}", path.display(), e))?;
        Ok(Some(config))
    }

    /// Update the default project in .track.toml (creates file if url/token are provided)
    pub fn update_default_project(project_short_name: &str) -> Result<()> {
        let path = local_track_config_path()?;
        if let Some(mut config) = Self::load_local_track_toml()? {
            config.default_project = Some(project_short_name.to_string());
            config.save(&path)?;
            Ok(())
        } else {
            Err(anyhow!(
                "No .track.toml found. Run 'track init' first, or create the file manually."
            ))
        }
    }

    /// Update the default backend in .track.toml
    pub fn update_backend(backend: Backend) -> Result<()> {
        let path = local_track_config_path()?;
        if let Some(mut config) = Self::load_local_track_toml()? {
            config.backend = Some(backend);
            config.save(&path)?;
            Ok(())
        } else {
            Err(anyhow!(
                "No .track.toml found. Run 'track init' first, or create the file manually."
            ))
        }
    }

    /// Get the configured backend, defaulting to YouTrack
    pub fn get_backend(&self) -> Backend {
        self.backend.unwrap_or_default()
    }
}

fn config_paths(explicit: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(path) = explicit {
        paths.push(path.to_path_buf());
        return paths;
    }

    // Load configs from lowest to highest priority
    // Later entries override earlier ones in figment merge
    // 1. Global: ~/.tracker-cli/.track.toml (lowest file priority)
    if let Some(path) = get_global_config_path() {
        paths.push(path);
    }
    // 2. Project: ./.track.toml (highest file priority)
    if let Some(path) = get_local_track_config_path()
        && !paths.contains(&path)
    {
        paths.push(path);
    }

    paths
}

/// Returns the path to the global config (~/.tracker-cli/.track.toml)
fn get_global_config_path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".tracker-cli").join(".track.toml"))
}

/// Returns the path to the local .track.toml file in the current directory
fn get_local_track_config_path() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|dir| dir.join(".track.toml"))
}

/// Returns the user's home directory
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Returns the path where `track init` will create the config file
pub fn local_track_config_path() -> Result<PathBuf> {
    std::env::current_dir()
        .map(|dir| dir.join(".track.toml"))
        .map_err(|e| anyhow!("Failed to get current directory: {}", e))
}

/// Returns the global config path (~/.tracker-cli/.track.toml)
pub fn global_config_path() -> Option<PathBuf> {
    get_global_config_path()
}

/// Returns the global config path, creating the parent directory if needed
pub fn global_config_path_ensure() -> Result<PathBuf> {
    let path = global_config_path()
        .ok_or_else(|| anyhow!("Could not determine home directory for global config"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow!("Failed to create directory {}: {}", parent.display(), e))?;
    }
    Ok(path)
}

/// Returns true if a .track.toml exists in the current directory (project context)
pub fn is_project_context() -> bool {
    get_local_track_config_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Returns the global cache directory (~/.tracker-cli/cache/)
pub fn global_cache_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".tracker-cli").join("cache"))
}

/// Load backend from the full config chain (global -> project -> env)
/// without requiring a backend argument.
pub fn resolve_backend() -> Backend {
    let mut figment = Figment::new().merge(Serialized::defaults(Config::default()));
    for path in config_paths(None) {
        if path.exists() {
            figment = figment.merge(Toml::file(path));
        }
    }
    figment = figment.merge(Env::prefixed("TRACKER_"));
    figment
        .extract::<Config>()
        .ok()
        .and_then(|c| c.backend)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_with_link_mappings() {
        let toml_str = r#"
backend = "jira"
[jira]
url = "https://test.atlassian.net"
email = "user@example.com"
token = "secret"

[jira.link_mappings]
depends = "Requires"
required = "Requires"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.jira.link_mappings.get("depends"),
            Some(&"Requires".to_string())
        );
        assert_eq!(
            config.jira.link_mappings.get("required"),
            Some(&"Requires".to_string())
        );
        assert!(!config.jira.is_empty());
    }

    #[test]
    fn test_config_without_link_mappings() {
        let toml_str = r#"
backend = "jira"
[jira]
url = "https://test.atlassian.net"
email = "user@example.com"
token = "secret"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.jira.link_mappings.is_empty());
    }

    #[test]
    fn test_config_youtrack_link_mappings() {
        let toml_str = r#"
backend = "youtrack"
[youtrack]
url = "https://yt.example.com"
token = "secret"

[youtrack.link_mappings]
depends = "Custom Depend"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.youtrack.link_mappings.get("depends"),
            Some(&"Custom Depend".to_string())
        );
    }

    #[test]
    fn test_config_gitlab_link_mappings() {
        let toml_str = r#"
backend = "gitlab"
[gitlab]
url = "https://gitlab.com/api/v4"
token = "secret"
project_id = "123"

[gitlab.link_mappings]
duplicates = "blocks"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.gitlab.link_mappings.get("duplicates"),
            Some(&"blocks".to_string())
        );
    }

    #[test]
    fn test_config_link_mappings_serialization_roundtrip() {
        let mut config = Config::default();
        config
            .jira
            .link_mappings
            .insert("depends".to_string(), "Requires".to_string());
        config.jira.url = Some("https://test.atlassian.net".to_string());

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            parsed.jira.link_mappings.get("depends"),
            Some(&"Requires".to_string())
        );
    }
}
