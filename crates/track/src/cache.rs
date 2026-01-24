use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracker_core::IssueTracker;

const CACHE_FILE_NAME: &str = ".tracker-cache.json";

/// Cached tracker context for AI assistants
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TrackerCache {
    /// Timestamp of last cache update
    pub updated_at: Option<String>,
    /// List of projects with their IDs
    pub projects: Vec<CachedProject>,
    /// Custom fields per project (keyed by project shortName)
    pub project_fields: Vec<ProjectFieldsCache>,
    /// Available tags
    pub tags: Vec<CachedTag>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedProject {
    pub id: String,
    pub short_name: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectFieldsCache {
    pub project_short_name: String,
    pub project_id: String,
    pub fields: Vec<CachedField>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedField {
    pub name: String,
    pub field_type: String,
    pub required: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedTag {
    pub id: String,
    pub name: String,
}

impl TrackerCache {
    /// Load cache from file
    pub fn load(cache_dir: Option<PathBuf>) -> Result<Self> {
        let path = Self::cache_path(cache_dir)?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read cache file: {}", path.display()))?;

        serde_json::from_str(&content).context("Failed to parse cache file")
    }

    /// Save cache to file
    pub fn save(&self, cache_dir: Option<PathBuf>) -> Result<()> {
        let path = Self::cache_path(cache_dir)?;

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write cache file: {}", path.display()))?;

        Ok(())
    }

    /// Get cache file path
    fn cache_path(cache_dir: Option<PathBuf>) -> Result<PathBuf> {
        if let Some(dir) = cache_dir {
            return Ok(dir.join(CACHE_FILE_NAME));
        }

        // Default to current directory
        Ok(PathBuf::from(CACHE_FILE_NAME))
    }

    /// Refresh cache from tracker API
    pub fn refresh(client: &dyn IssueTracker) -> Result<Self> {
        let mut cache = Self {
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Self::default()
        };

        // Fetch projects
        let projects = client.list_projects().context("Failed to fetch projects")?;
        cache.projects = projects
            .iter()
            .map(|p| CachedProject {
                id: p.id.clone(),
                short_name: p.short_name.clone(),
                name: p.name.clone(),
                description: p.description.clone(),
            })
            .collect();

        // Fetch custom fields for each project
        for project in &projects {
            if let Ok(fields) = client.get_project_custom_fields(&project.id) {
                let cached_fields: Vec<CachedField> = fields
                    .iter()
                    .map(|f| CachedField {
                        name: f.name.clone(),
                        field_type: f.field_type.clone(),
                        required: f.required,
                    })
                    .collect();

                cache.project_fields.push(ProjectFieldsCache {
                    project_short_name: project.short_name.clone(),
                    project_id: project.id.clone(),
                    fields: cached_fields,
                });
            }
        }

        // Fetch tags
        if let Ok(tags) = client.list_tags() {
            cache.tags = tags
                .iter()
                .map(|t| CachedTag {
                    id: t.id.clone(),
                    name: t.name.clone(),
                })
                .collect();
        }

        Ok(cache)
    }

    /// Get project ID by shortName (from cache)
    #[allow(dead_code)]
    pub fn get_project_id(&self, short_name: &str) -> Option<&str> {
        self.projects
            .iter()
            .find(|p| p.short_name.eq_ignore_ascii_case(short_name))
            .map(|p| p.id.as_str())
    }

    /// Get fields for a project (from cache)
    #[allow(dead_code)]
    pub fn get_project_fields(&self, short_name: &str) -> Option<&[CachedField]> {
        self.project_fields
            .iter()
            .find(|pf| pf.project_short_name.eq_ignore_ascii_case(short_name))
            .map(|pf| pf.fields.as_slice())
    }

    /// Get tag ID by name (from cache)
    #[allow(dead_code)]
    pub fn get_tag_id(&self, name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
            .map(|t| t.id.as_str())
    }
}
