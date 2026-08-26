use crate::cli::Backend;
use anyhow::{Result, anyhow};
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use tracker_core::unicode_eq_ignore_case;

use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Main configuration structure supporting multiple backends
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Config {
    /// Default backend to use (youtrack, jira, github, gitlab, or linear)
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
    /// Linear-specific configuration
    #[serde(default, skip_serializing_if = "LinearConfig::is_empty")]
    pub linear: LinearConfig,
    /// Raw `[workflow_pack]` table from this config file.
    ///
    /// Kept as TOML so ordinary [`Config::load`] / [`Config::save`] and
    /// `config set` do not fail on, or drop, unknown or invalid pack keys.
    /// Pack-consuming commands parse the selected file via [`load_workflow_pack`]
    /// (whole-pack source selection; not field-merged across files).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_pack: Option<toml::Value>,
}

/// Source of the effective workflow pack. Not serialized into config files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowPackSource {
    Repo,
    User,
    Explicit,
}

impl WorkflowPackSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Repo => "repo",
            Self::User => "user",
            Self::Explicit => "explicit",
        }
    }
}

impl std::fmt::Display for WorkflowPackSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed `[workflow_pack]` used by pack-consuming commands.
///
/// Ordinary config load/save keep the section as raw TOML on [`Config`].
/// [`parse_workflow_pack_table`] is the strict parser (unknown fields denied).
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Default)]
pub struct WorkflowPack {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_project: Option<String>,
    #[serde(default)]
    pub queries: Vec<WorkflowPackQuery>,
}

/// One `[[workflow_pack.queries]]` record.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Default)]
pub struct WorkflowPackQuery {
    pub name: String,
    pub description: String,
    pub query: String,
}

/// A semantically valid pack plus the file it was selected from.
#[derive(Debug, Clone)]
pub struct LoadedWorkflowPack {
    pub source: WorkflowPackSource,
    pub pack: WorkflowPack,
}

/// Result of whole-pack discovery for the current process.
#[derive(Debug, Clone)]
pub enum WorkflowPackState {
    None,
    Valid(LoadedWorkflowPack),
    Invalid {
        source: WorkflowPackSource,
        errors: Vec<String>,
    },
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

    /// Connection-relevant keys only: cosmetic settings like link_mappings
    /// must not make a backend count as configured.
    pub fn has_connection_config(&self) -> bool {
        self.url.is_some() || self.token.is_some()
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

    /// Connection-relevant keys only (excludes link_mappings).
    pub fn has_connection_config(&self) -> bool {
        self.url.is_some() || self.email.is_some() || self.token.is_some()
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

    /// Connection-relevant keys (GitHub has no cosmetic-only keys today, but
    /// this keeps backend enumeration uniform across sections).
    pub fn has_connection_config(&self) -> bool {
        !self.is_empty()
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

    /// Connection-relevant keys only (excludes link_mappings).
    pub fn has_connection_config(&self) -> bool {
        self.token.is_some()
            || self.url.is_some()
            || self.project_id.is_some()
            || self.namespace.is_some()
    }
}

/// Linear-specific configuration
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct LinearConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Linear GraphQL API URL (defaults to https://api.linear.app/graphql)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    /// Linear workspace/web URL used by `track open` (e.g. https://linear.app/acme)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Default Linear team key/name/id. Falls back to top-level default_project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_team: Option<String>,
    /// Default Linear project association for issue creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_linear_project: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub link_mappings: HashMap<String, String>,
}

impl LinearConfig {
    pub fn is_empty(&self) -> bool {
        self.token.is_none()
            && self.api_url.is_none()
            && self.url.is_none()
            && self.default_team.is_none()
            && self.default_linear_project.is_none()
            && self.link_mappings.is_empty()
    }

    /// Connection-relevant keys only (excludes link_mappings).
    pub fn has_connection_config(&self) -> bool {
        self.token.is_some()
            || self.api_url.is_some()
            || self.url.is_some()
            || self.default_team.is_some()
            || self.default_linear_project.is_some()
    }
}

fn map_youtrack_env_key(key: &str) -> Option<&'static str> {
    match key.to_ascii_lowercase().as_str() {
        "url" => Some("youtrack.url"),
        "token" => Some("youtrack.token"),
        _ => None,
    }
}

fn map_jira_env_key(key: &str) -> Option<&'static str> {
    match key.to_ascii_lowercase().as_str() {
        "url" => Some("jira.url"),
        "email" => Some("jira.email"),
        "token" => Some("jira.token"),
        _ => None,
    }
}

fn map_github_env_key(key: &str) -> Option<&'static str> {
    match key.to_ascii_lowercase().as_str() {
        "token" => Some("github.token"),
        "owner" => Some("github.owner"),
        "repo" => Some("github.repo"),
        "api_url" => Some("github.api_url"),
        _ => None,
    }
}

fn map_gitlab_env_key(key: &str) -> Option<&'static str> {
    match key.to_ascii_lowercase().as_str() {
        "token" => Some("gitlab.token"),
        "url" => Some("gitlab.url"),
        "project_id" => Some("gitlab.project_id"),
        "namespace" => Some("gitlab.namespace"),
        _ => None,
    }
}

fn map_linear_env_key(key: &str) -> Option<&'static str> {
    match key.to_ascii_lowercase().as_str() {
        "token" => Some("linear.token"),
        "api_url" => Some("linear.api_url"),
        "url" => Some("linear.url"),
        "default_team" => Some("linear.default_team"),
        "default_project" => Some("linear.default_linear_project"),
        _ => None,
    }
}

impl Config {
    pub fn load(config_path: Option<PathBuf>, backend: Backend) -> Result<Self> {
        let mut config = Self::load_raw(config_path)?;

        // Merge backend-specific config with global config
        config.apply_backend_config(backend);

        Ok(config)
    }

    /// Load the merged configuration (files + env) WITHOUT collapsing any
    /// backend-specific section into the flat url/token fields.
    ///
    /// Use this when the per-backend sections themselves matter, e.g. to
    /// enumerate which backends are configured (`track doctor --all-backends`).
    pub fn load_raw(config_path: Option<PathBuf>) -> Result<Self> {
        let mut figment = Figment::new().merge(Serialized::defaults(Config::default()));

        let explicit_path = config_path.as_deref();
        if let Some(path) = explicit_path
            && !path.exists()
        {
            return Err(anyhow!("Config file not found: {}", path.display()));
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
                if let Some(mapped) = map_youtrack_env_key(key.as_str()) {
                    mapped.into()
                } else {
                    key.into()
                }
            }))
            .merge(Env::prefixed("JIRA_").map(|key| {
                // Map JIRA_URL -> jira.url for nested config
                if let Some(mapped) = map_jira_env_key(key.as_str()) {
                    mapped.into()
                } else {
                    key.into()
                }
            }))
            .merge(Env::prefixed("GITHUB_").map(|key| {
                if let Some(mapped) = map_github_env_key(key.as_str()) {
                    mapped.into()
                } else {
                    key.into()
                }
            }))
            .merge(Env::prefixed("GITLAB_").map(|key| {
                if let Some(mapped) = map_gitlab_env_key(key.as_str()) {
                    mapped.into()
                } else {
                    key.into()
                }
            }))
            .merge(Env::prefixed("LINEAR_").map(|key| {
                if let Some(mapped) = map_linear_env_key(key.as_str()) {
                    mapped.into()
                } else {
                    key.into()
                }
            }));

        let config: Config = figment
            .extract()
            .map_err(|e| anyhow!("Failed to load config: {}", e))?;

        Ok(config)
    }

    /// Enumerate backends that have any configuration present.
    ///
    /// Must be called on a raw config (see [`Config::load_raw`]): once
    /// `apply_backend_config` has collapsed a backend's section into the flat
    /// url/token fields, that section is partially consumed. A backend counts
    /// as configured when its nested section has connection-relevant keys
    /// (link_mappings alone don't count), or when it is the default backend
    /// and only flat url/token settings exist.
    pub fn configured_backends(&self) -> Vec<Backend> {
        let mut found = Vec::new();
        if self.youtrack.has_connection_config() {
            found.push(Backend::YouTrack);
        }
        if self.jira.has_connection_config() {
            found.push(Backend::Jira);
        }
        if self.github.has_connection_config() {
            found.push(Backend::GitHub);
        }
        if self.gitlab.has_connection_config() {
            found.push(Backend::GitLab);
        }
        if self.linear.has_connection_config() {
            found.push(Backend::Linear);
        }

        // Single-backend setups often only set the flat url/token keys; those
        // belong to the default backend.
        let default = self.get_backend();
        if !found.contains(&default) && (self.url.is_some() || self.token.is_some()) {
            found.push(default);
        }

        Backend::ALL
            .into_iter()
            .filter(|b| found.contains(b))
            .collect()
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
            Backend::Linear => {
                if let Some(u) = self.linear.url.take() {
                    self.url = Some(u);
                } else {
                    let is_linear_url = self
                        .url
                        .as_deref()
                        .is_some_and(|u| u.to_lowercase().contains("linear"));
                    if !is_linear_url {
                        self.url = None;
                    }
                }
                if let Some(t) = self.linear.token.take() {
                    self.token = Some(t);
                }
                if self.default_project.is_none()
                    && let Some(team) = self.linear.default_team.clone()
                {
                    self.default_project = Some(team);
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
            Backend::Linear => "Linear",
        };

        if backend != Backend::Linear && self.url.is_none() {
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
        if backend == Backend::GitHub {
            if self.github.owner.is_none() {
                return Err(anyhow!(
                    "GitHub owner not configured. Set via 'track config set github.owner <OWNER>' or GITHUB_OWNER env var"
                ));
            }
            if self.github.repo.is_none() {
                return Err(anyhow!(
                    "GitHub repo not configured. Set via 'track config set github.repo <REPO>' or GITHUB_REPO env var"
                ));
            }
        }
        if backend == Backend::GitLab && self.gitlab.project_id.is_none() {
            return Err(anyhow!(
                "GitLab project_id not configured. Set via 'track config set gitlab.project_id <ID>' or GITLAB_PROJECT_ID env var"
            ));
        }
        Ok(())
    }

    /// Save configuration to a TOML file
    pub fn save(&self, path: &Path) -> Result<()> {
        let toml_string = toml::to_string_pretty(self)
            .map_err(|e| anyhow!("Failed to serialize config: {}", e))?;

        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);

        let mut file = options
            .open(path)
            .map_err(|e| anyhow!("Failed to open config file: {}", e))?;
        file.write_all(toml_string.as_bytes())
            .map_err(|e| anyhow!("Failed to write config file: {}", e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        }
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

    /// Link-type mappings (canonical keyword -> backend link type name) for
    /// the given backend. GitHub has no configurable link types.
    pub fn link_mappings_for(&self, backend: Backend) -> &HashMap<String, String> {
        static NO_MAPPINGS: std::sync::LazyLock<HashMap<String, String>> =
            std::sync::LazyLock::new(HashMap::new);
        match backend {
            Backend::YouTrack => &self.youtrack.link_mappings,
            Backend::Jira => &self.jira.link_mappings,
            Backend::GitHub => &NO_MAPPINGS,
            Backend::GitLab => &self.gitlab.link_mappings,
            Backend::Linear => &self.linear.link_mappings,
        }
    }
}

pub(crate) fn config_paths(explicit: Option<&Path>) -> Vec<PathBuf> {
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
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(parent)
            .map_err(|e| anyhow!("Failed to create directory {}: {}", parent.display(), e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
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

/// Load the effective workflow pack using whole-pack source selection.
///
/// Path order reuses the same helpers as [`config_paths`],
/// [`local_track_config_path`], and [`global_config_path`]:
/// 1. `--config` / `TRACKER_CONFIG` inspects only that file (`Explicit`)
/// 2. else `./.track.toml` if it contains `[workflow_pack]` (`Repo`)
/// 3. else `~/.tracker-cli/.track.toml` if it contains `[workflow_pack]` (`User`)
/// 4. else no pack
///
/// Repo replaces user; query arrays and metadata are never merged.
pub fn load_workflow_pack(explicit: Option<&Path>) -> Result<WorkflowPackState> {
    let Some((path, source)) = select_workflow_pack_file(explicit)? else {
        return Ok(WorkflowPackState::None);
    };
    if !path.exists() {
        if source == WorkflowPackSource::Explicit {
            return Err(anyhow!("Config file not found: {}", path.display()));
        }
        return Ok(WorkflowPackState::None);
    }

    let table = read_toml_table(&path)?;
    let Some(pack_value) = table.get("workflow_pack") else {
        return Ok(WorkflowPackState::None);
    };

    match parse_workflow_pack_table(pack_value) {
        Ok(pack) => match validate_workflow_pack(&pack) {
            Ok(()) => Ok(WorkflowPackState::Valid(LoadedWorkflowPack {
                source,
                pack,
            })),
            Err(errors) => Ok(WorkflowPackState::Invalid { source, errors }),
        },
        Err(errors) => Ok(WorkflowPackState::Invalid { source, errors }),
    }
}

/// Require a valid pack for pack-consuming commands. `None` means no pack.
pub fn require_valid_workflow_pack(explicit: Option<&Path>) -> Result<Option<LoadedWorkflowPack>> {
    match load_workflow_pack(explicit)? {
        WorkflowPackState::None => Ok(None),
        WorkflowPackState::Valid(loaded) => Ok(Some(loaded)),
        WorkflowPackState::Invalid { errors, .. } => Err(anyhow!("{}", errors.join("\n"))),
    }
}

fn select_workflow_pack_file(
    explicit: Option<&Path>,
) -> Result<Option<(PathBuf, WorkflowPackSource)>> {
    if let Some(path) = explicit {
        return Ok(Some((path.to_path_buf(), WorkflowPackSource::Explicit)));
    }

    if let Some(path) = get_local_track_config_path()
        && path.exists()
        && toml_has_workflow_pack(&path)?
    {
        return Ok(Some((path, WorkflowPackSource::Repo)));
    }

    if let Some(path) = get_global_config_path()
        && path.exists()
        && toml_has_workflow_pack(&path)?
    {
        return Ok(Some((path, WorkflowPackSource::User)));
    }

    Ok(None)
}

fn read_toml_table(path: &Path) -> Result<toml::Table> {
    let content = fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read {}: {}", path.display(), e))?;
    content
        .parse::<toml::Table>()
        .map_err(|e| anyhow!("Failed to parse {}: {}", path.display(), e))
}

fn toml_has_workflow_pack(path: &Path) -> Result<bool> {
    Ok(read_toml_table(path)?.contains_key("workflow_pack"))
}

/// Parse a `[workflow_pack]` table, rejecting unknown fields.
fn parse_workflow_pack_table(value: &toml::Value) -> Result<WorkflowPack, Vec<String>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StrictPack {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        default_project: Option<String>,
        #[serde(default)]
        queries: Vec<StrictQuery>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StrictQuery {
        name: String,
        description: String,
        query: String,
    }

    let strict: StrictPack = value
        .clone()
        .try_into()
        .map_err(|e: toml::de::Error| vec![format!("Invalid workflow_pack: {e}")])?;

    Ok(WorkflowPack {
        name: strict.name,
        description: strict.description,
        default_project: strict.default_project,
        queries: strict
            .queries
            .into_iter()
            .map(|q| WorkflowPackQuery {
                name: q.name,
                description: q.description,
                query: q.query,
            })
            .collect(),
    })
}

fn is_valid_query_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {
            chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        }
        _ => false,
    }
}

/// Semantic validation for a parsed pack (required fields, name syntax, dupes).
pub fn validate_workflow_pack(pack: &WorkflowPack) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if pack.name.trim().is_empty() {
        errors.push("workflow pack name must be non-empty".to_string());
    }
    if let Some(description) = &pack.description
        && description.trim().is_empty()
    {
        errors.push("workflow pack description must be non-empty when present".to_string());
    }
    if let Some(project) = &pack.default_project
        && project.trim().is_empty()
    {
        errors.push("workflow pack default_project must be non-empty when present".to_string());
    }
    if pack.queries.is_empty() {
        errors.push("workflow_pack.queries must contain at least one query".to_string());
    }

    for (index, query) in pack.queries.iter().enumerate() {
        if pack
            .queries
            .iter()
            .take(index)
            .any(|other| unicode_eq_ignore_case(&other.name, &query.name))
        {
            errors.push(format!(
                "Duplicate query name '{}' in workflow pack (already declared)",
                query.name
            ));
        }
    }

    for query in &pack.queries {
        if query.name.trim().is_empty() {
            errors.push("workflow pack query name must be non-empty".to_string());
        } else if !is_valid_query_name(&query.name) {
            errors.push(format!(
                "Invalid query name '{}': must match ^[a-z][a-z0-9_]*$",
                query.name
            ));
        }
        if query.description.trim().is_empty() {
            errors.push(format!(
                "query '{}' description must be non-empty",
                query.name
            ));
        }
        if query.query.trim().is_empty() {
            errors.push(format!("query '{}' query must be non-empty", query.name));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Whether `name` matches a pack query case-insensitively.
pub fn pack_query_named<'a>(pack: &'a WorkflowPack, name: &str) -> Option<&'a WorkflowPackQuery> {
    pack.queries
        .iter()
        .find(|query| unicode_eq_ignore_case(&query.name, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn config_save_uses_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.toml");
        Config::default().save(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

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

    #[test]
    fn test_backend_env_key_mappers_are_case_insensitive() {
        assert_eq!(map_youtrack_env_key("URL"), Some("youtrack.url"));
        assert_eq!(map_youtrack_env_key("Token"), Some("youtrack.token"));
        assert_eq!(map_jira_env_key("EMAIL"), Some("jira.email"));
        assert_eq!(map_github_env_key("OWNER"), Some("github.owner"));
        assert_eq!(map_github_env_key("API_URL"), Some("github.api_url"));
        assert_eq!(map_gitlab_env_key("PROJECT_ID"), Some("gitlab.project_id"));
        assert_eq!(map_linear_env_key("TOKEN"), Some("linear.token"));
        assert_eq!(map_linear_env_key("API_URL"), Some("linear.api_url"));
        assert_eq!(
            map_linear_env_key("DEFAULT_TEAM"),
            Some("linear.default_team")
        );
        assert_eq!(
            map_linear_env_key("DEFAULT_PROJECT"),
            Some("linear.default_linear_project")
        );
        assert_eq!(map_linear_env_key("UNKNOWN"), None);
    }

    #[test]
    fn test_validate_github_requires_owner_and_repo() {
        let config = Config {
            url: Some("https://api.github.com".to_string()),
            token: Some("secret".to_string()),
            github: GitHubConfig {
                repo: Some("repo".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let err = config.validate(Backend::GitHub).unwrap_err();
        assert!(
            err.to_string().contains("GitHub owner not configured"),
            "expected missing owner error, got: {err}"
        );

        let config = Config {
            url: Some("https://api.github.com".to_string()),
            token: Some("secret".to_string()),
            github: GitHubConfig {
                owner: Some("org".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };

        let err = config.validate(Backend::GitHub).unwrap_err();
        assert!(
            err.to_string().contains("GitHub repo not configured"),
            "expected missing repo error, got: {err}"
        );
    }

    #[test]
    fn test_configured_backends_from_sections() {
        let toml_str = r#"
backend = "youtrack"
[youtrack]
url = "https://yt.example.com"
token = "yt-secret"

[gitlab]
url = "https://gitlab.com/api/v4"
token = "gl-secret"
project_id = "123"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.configured_backends(),
            vec![Backend::YouTrack, Backend::GitLab]
        );
    }

    #[test]
    fn test_configured_backends_flat_keys_count_for_default_backend() {
        let toml_str = r#"
backend = "jira"
url = "https://test.atlassian.net"
token = "secret"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.configured_backends(), vec![Backend::Jira]);
    }

    #[test]
    fn test_configured_backends_flat_keys_default_to_youtrack() {
        let toml_str = r#"
url = "https://yt.example.com"
token = "secret"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.configured_backends(), vec![Backend::YouTrack]);
    }

    #[test]
    fn test_configured_backends_empty_config() {
        let config = Config::default();
        assert!(config.configured_backends().is_empty());
    }

    #[test]
    fn test_configured_backends_ignores_link_mappings_only_sections() {
        // A cosmetic link_mappings table alone must not enumerate the backend
        // (it has no url/token, so auditing it would always fail).
        let toml_str = r#"
[youtrack.link_mappings]
"parent for" = "subtask of"

[gitlab]
url = "https://gitlab.com/api/v4"
token = "gl-secret"
project_id = "123"

[linear.link_mappings]
blocks = "blocked by"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.configured_backends(), vec![Backend::GitLab]);
    }

    #[test]
    fn test_configured_backends_stable_order() {
        let toml_str = r#"
[linear]
token = "lin-secret"

[jira]
url = "https://test.atlassian.net"
email = "user@example.com"
token = "j-secret"

[github]
token = "gh-secret"
owner = "org"
repo = "repo"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.configured_backends(),
            vec![Backend::Jira, Backend::GitHub, Backend::Linear]
        );
    }

    #[test]
    fn test_validate_gitlab_requires_project_id() {
        let config = Config {
            url: Some("https://gitlab.com/api/v4".to_string()),
            token: Some("secret".to_string()),
            ..Default::default()
        };

        let err = config.validate(Backend::GitLab).unwrap_err();
        assert!(
            err.to_string().contains("GitLab project_id not configured"),
            "expected missing project_id error, got: {err}"
        );
    }

    fn sample_pack_toml() -> &'static str {
        r#"
backend = "youtrack"

[workflow_pack]
name = "Orek backlog"
description = "Project-local views."
default_project = "DEMO"

[[workflow_pack.queries]]
name = "ready"
description = "Ready work."
query = "project: {PROJECT} #Unresolved State: {Ready}"
"#
    }

    fn pack_table_str<'a>(pack: &'a toml::Value, key: &str) -> Option<&'a str> {
        pack.get(key).and_then(|v| v.as_str())
    }

    #[test]
    fn workflow_pack_roundtrips_through_config_save() {
        let config: Config = toml::from_str(sample_pack_toml()).unwrap();
        let pack = config.workflow_pack.as_ref().unwrap();
        assert_eq!(pack_table_str(pack, "name"), Some("Orek backlog"));
        assert_eq!(
            pack.get("queries")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(1)
        );

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".track.toml");
        config.save(&path).unwrap();
        let reloaded = Config::load_from_path(&path).unwrap().unwrap();
        let pack = reloaded.workflow_pack.as_ref().unwrap();
        assert_eq!(pack_table_str(pack, "name"), Some("Orek backlog"));
        assert_eq!(
            pack.get("queries")
                .and_then(|v| v.as_array())
                .and_then(|queries| queries[0].get("name"))
                .and_then(|v| v.as_str()),
            Some("ready")
        );
    }

    #[test]
    fn ordinary_config_load_accepts_structural_pack_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".track.toml");
        std::fs::write(
            &path,
            r#"
backend = "youtrack"

[workflow_pack]
name = "Structural error pack"
queries = "not-an-array"
"#,
        )
        .unwrap();

        let from_path = Config::load_from_path(&path).unwrap().unwrap();
        assert_eq!(from_path.backend, Some(Backend::YouTrack));
        assert_eq!(
            pack_table_str(from_path.workflow_pack.as_ref().unwrap(), "queries"),
            Some("not-an-array")
        );

        let loaded = Config::load(Some(path), Backend::YouTrack).unwrap();
        assert_eq!(loaded.backend, Some(Backend::YouTrack));
        assert_eq!(
            pack_table_str(loaded.workflow_pack.as_ref().unwrap(), "queries"),
            Some("not-an-array")
        );
    }

    #[test]
    fn config_save_preserves_unknown_or_invalid_pack_keys() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".track.toml");
        std::fs::write(
            &path,
            r#"
backend = "youtrack"

[workflow_pack]
name = "Extra fields pack"
extra = 1

[[workflow_pack.queries]]
name = "ready"
description = "Ready work."
query = "project: {PROJECT} #Unresolved State: {Ready}"
unexpected = true
retries = "not-a-number"
"#,
        )
        .unwrap();

        let mut config = Config::load_from_path(&path).unwrap().unwrap();
        config.default_project = Some("DEMO".to_string());
        config.save(&path).unwrap();

        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(
            saved.contains("default_project") && saved.contains("DEMO"),
            "typed edit must be written, got:\n{saved}"
        );
        assert!(
            saved.contains("extra") && saved.contains('1'),
            "unknown top-level pack key must survive, got:\n{saved}"
        );
        assert!(
            saved.contains("unexpected"),
            "nested unknown query field must survive, got:\n{saved}"
        );
        assert!(
            saved.contains("retries") && saved.contains("not-a-number"),
            "invalid nested query type must survive, got:\n{saved}"
        );
    }

    #[test]
    fn parse_workflow_pack_rejects_unknown_field() {
        let value: toml::Value = toml::from_str(
            r#"
name = "Broken"
not_a_real_pack_field = "typo"
queries = []
"#,
        )
        .unwrap();
        let err = parse_workflow_pack_table(&value).unwrap_err().join(" ");
        assert!(
            err.contains("not_a_real_pack_field") || err.to_ascii_lowercase().contains("unknown"),
            "expected unknown-field error, got {err}"
        );
    }

    #[test]
    fn validate_workflow_pack_rejects_empty_name() {
        let pack = WorkflowPack {
            name: String::new(),
            description: Some("desc".to_string()),
            default_project: None,
            queries: vec![WorkflowPackQuery {
                name: "ready".to_string(),
                description: "Ready".to_string(),
                query: "project: {PROJECT}".to_string(),
            }],
        };
        let errors = validate_workflow_pack(&pack).unwrap_err().join(" ");
        assert!(errors.contains("name"));
        assert!(
            errors.contains("empty") || errors.contains("non-empty"),
            "got {errors}"
        );
    }

    #[test]
    fn validate_workflow_pack_rejects_duplicate_names() {
        let pack = WorkflowPack {
            name: "Dupes".to_string(),
            description: None,
            default_project: None,
            queries: vec![
                WorkflowPackQuery {
                    name: "ready".to_string(),
                    description: "Lower".to_string(),
                    query: "q1".to_string(),
                },
                WorkflowPackQuery {
                    name: "Ready".to_string(),
                    description: "Upper".to_string(),
                    query: "q2".to_string(),
                },
            ],
        };
        let errors = validate_workflow_pack(&pack).unwrap_err().join(" ");
        assert!(errors.to_ascii_lowercase().contains("ready"));
        assert!(
            errors.to_ascii_lowercase().contains("duplicate")
                || errors.to_ascii_lowercase().contains("already"),
            "got {errors}"
        );
    }

    #[test]
    fn load_workflow_pack_uses_explicit_file_only() {
        let dir = tempfile::TempDir::new().unwrap();
        let pack = dir.path().join("pack.toml");
        std::fs::write(&pack, sample_pack_toml()).unwrap();
        match load_workflow_pack(Some(&pack)).unwrap() {
            WorkflowPackState::Valid(loaded) => {
                assert_eq!(loaded.source, WorkflowPackSource::Explicit);
                assert_eq!(loaded.pack.name, "Orek backlog");
                assert_eq!(loaded.pack.queries[0].name, "ready");
            }
            other => panic!("expected valid pack, got {other:?}"),
        }
    }

    #[test]
    fn is_valid_query_name_matches_snake_case() {
        assert!(is_valid_query_name("ready"));
        assert!(is_valid_query_name("my_issues"));
        assert!(is_valid_query_name("a1"));
        assert!(!is_valid_query_name("Ready"));
        assert!(!is_valid_query_name("ready-now"));
        assert!(!is_valid_query_name("1ready"));
        assert!(!is_valid_query_name(""));
    }
}
