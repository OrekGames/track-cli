use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const LOCAL_CONFIG_FILE_NAME: &str = ".track-config.json";

/// Local configuration stored in the current directory
/// Stores user preferences like default project
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct LocalConfig {
    /// Default project ID (internal ID like "0-2")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_project_id: Option<String>,
    /// Default project shortName for display (like "OGIT")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_project_name: Option<String>,
}

impl LocalConfig {
    /// Load local config from the current directory
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read local config: {}", path.display()))?;

        serde_json::from_str(&content).context("Failed to parse local config")
    }

    /// Save local config to the current directory
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write local config: {}", path.display()))?;
        Ok(())
    }

    /// Get the config file path
    pub fn config_path() -> Result<PathBuf> {
        Ok(std::env::current_dir()?.join(LOCAL_CONFIG_FILE_NAME))
    }

    /// Set the default project
    pub fn set_default_project(&mut self, project_id: String, project_name: String) {
        self.default_project_id = Some(project_id);
        self.default_project_name = Some(project_name);
    }

    /// Clear the default project
    pub fn clear_default_project(&mut self) {
        self.default_project_id = None;
        self.default_project_name = None;
    }

    /// Check if config is empty (no settings)
    pub fn is_empty(&self) -> bool {
        self.default_project_id.is_none()
    }

    /// Delete the config file
    pub fn delete() -> Result<()> {
        let path = Self::config_path()?;
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to delete local config: {}", path.display()))?;
        }
        Ok(())
    }
}
