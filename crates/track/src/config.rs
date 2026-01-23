use crate::cli::Backend;
use anyhow::{anyhow, Result};
use directories::{BaseDirs, ProjectDirs};
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Main configuration structure supporting multiple backends
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Config {
    /// Global URL override (applies to any backend)
    pub url: Option<String>,
    /// Global token override (applies to any backend)
    pub token: Option<String>,
    /// YouTrack-specific configuration
    #[serde(default)]
    pub youtrack: BackendConfig,
    // Future: jira, linear, github, etc.
}

/// Backend-specific configuration
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct BackendConfig {
    pub url: Option<String>,
    pub token: Option<String>,
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

        // Support both TRACKER_* and legacy YOUTRACK_* environment variables
        figment = figment
            .merge(Env::prefixed("TRACKER_"))
            .merge(Env::prefixed("YOUTRACK_").map(|key| {
                // Map YOUTRACK_URL -> youtrack.url for nested config
                match key.as_str() {
                    "url" => "youtrack.url".into(),
                    "token" => "youtrack.token".into(),
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
        Ok(())
    }
}

fn config_paths(explicit: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(path) = explicit {
        paths.push(path.to_path_buf());
        return paths;
    }

    if let Some(path) = get_project_config_path() {
        push_unique(&mut paths, path);
    }
    if let Some(path) = get_xdg_config_path() {
        push_unique(&mut paths, path);
    }
    if let Some(path) = get_local_config_path() {
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
