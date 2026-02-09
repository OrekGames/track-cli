use crate::cli::Backend;
use anyhow::{anyhow, Result};
use directories::{BaseDirs, ProjectDirs};
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Main configuration structure supporting multiple backends
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Config {
    /// Default backend to use (youtrack or jira)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
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
    // Future: linear, github, etc.
}

/// Backend-specific configuration
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct BackendConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl BackendConfig {
    pub fn is_empty(&self) -> bool {
        self.url.is_none() && self.token.is_none()
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
}

impl JiraConfig {
    pub fn is_empty(&self) -> bool {
        self.url.is_none() && self.email.is_none() && self.token.is_none()
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
                // If global url/token are set, they override backend-specific ones
                // But if global ones are not set, use backend-specific ones
                if self.url.is_none() {
                    self.url = self.youtrack.url.take();
                }
                if self.token.is_none() {
                    self.token = self.youtrack.token.take();
                }
            }
            Backend::Jira => {
                // Jira needs url, email, and token
                if self.url.is_none() {
                    self.url = self.jira.url.take();
                }
                if self.email.is_none() {
                    self.email = self.jira.email.take();
                }
                if self.token.is_none() {
                    self.token = self.jira.token.take();
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
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
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
            let backend_str = match backend {
                Backend::YouTrack => "youtrack",
                Backend::Jira => "jira",
            };
            config.backend = Some(backend_str.to_string());
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
        match self.backend.as_deref() {
            Some("jira") | Some("j") => Backend::Jira,
            _ => Backend::YouTrack, // Default
        }
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
    if let Some(path) = get_project_config_path() {
        push_unique(&mut paths, path);
    }
    if let Some(path) = get_xdg_config_path() {
        push_unique(&mut paths, path);
    }
    if let Some(path) = get_install_dir_config_path() {
        push_unique(&mut paths, path);
    }
    if let Some(path) = get_local_config_path() {
        push_unique(&mut paths, path);
    }
    // .track.toml has highest priority (project-specific)
    if let Some(path) = get_local_track_config_path() {
        push_unique(&mut paths, path);
    }

    paths
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

fn get_project_config_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "track").map(|d| d.config_dir().join("config.toml"))
}

fn get_xdg_config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(dir).join("track").join("config.toml"));
    }

    BaseDirs::new().map(|dirs| {
        dirs.home_dir()
            .join(".config")
            .join("track")
            .join("config.toml")
    })
}

fn get_local_config_path() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|dir| dir.join("config.toml"))
}

/// Returns the path to the global install directory config (~/.tracker-cli/.track.toml)
fn get_install_dir_config_path() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| {
        dirs.home_dir()
            .join(".tracker-cli")
            .join(".track.toml")
    })
}

/// Returns the path to the local .track.toml file in the current directory
fn get_local_track_config_path() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|dir| dir.join(".track.toml"))
}

/// Returns the path where `track init` will create the config file
pub fn local_track_config_path() -> Result<PathBuf> {
    std::env::current_dir()
        .map(|dir| dir.join(".track.toml"))
        .map_err(|e| anyhow!("Failed to get current directory: {}", e))
}
