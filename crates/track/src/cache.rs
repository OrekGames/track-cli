use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::thread;

#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use tracker_core::{IssueTracker, KnowledgeBase};

/// Creates a directory and its parents with secure permissions (0o700 on Unix)
fn create_dir_all_secure(path: &Path) -> Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

const CACHE_DIR_NAME: &str = ".tracker-cache";
const CACHE_VERSION: u32 = 2;
const MAX_RECENT_ISSUES: usize = 50;
const DEFAULT_CACHE_REFRESH_CONCURRENCY: usize = 4;
const MIN_CACHE_REFRESH_CONCURRENCY: usize = 1;
const MAX_CACHE_REFRESH_CONCURRENCY: usize = 16;
const MIN_PARALLEL_PROJECT_META_SHARDS: usize = 64;

fn cache_refresh_concurrency() -> usize {
    let configured = env::var("TRACK_CACHE_REFRESH_CONCURRENCY")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok());
    clamp_cache_refresh_concurrency(configured)
}

fn clamp_cache_refresh_concurrency(configured: Option<usize>) -> usize {
    configured
        .unwrap_or(DEFAULT_CACHE_REFRESH_CONCURRENCY)
        .clamp(MIN_CACHE_REFRESH_CONCURRENCY, MAX_CACHE_REFRESH_CONCURRENCY)
}

fn project_metadata_worker_count(project_count: usize, configured: Option<usize>) -> usize {
    if project_count < MIN_PARALLEL_PROJECT_META_SHARDS {
        return 1;
    }

    clamp_cache_refresh_concurrency(configured).min(project_count)
}

fn configured_project_metadata_worker_count(project_count: usize) -> usize {
    project_metadata_worker_count(project_count, Some(cache_refresh_concurrency()))
}

fn bounded_parallel_map_indexed<T, R, F>(items: &[T], concurrency: usize, job: F) -> Vec<(usize, R)>
where
    T: Sync,
    R: Send,
    F: Fn(usize, &T) -> Option<R> + Sync,
{
    if items.is_empty() {
        return Vec::new();
    }

    let worker_count = concurrency
        .clamp(MIN_CACHE_REFRESH_CONCURRENCY, MAX_CACHE_REFRESH_CONCURRENCY)
        .min(items.len());

    if worker_count == 1 {
        return items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| job(index, item).map(|result| (index, result)))
            .collect();
    }

    let next_index = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel();

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let tx = tx.clone();
            let next_index = &next_index;
            let job = &job;

            scope.spawn(move || {
                loop {
                    let index = next_index.fetch_add(1, Ordering::Relaxed);
                    if index >= items.len() {
                        break;
                    }

                    if let Some(result) = job(index, &items[index]) {
                        let _ = tx.send((index, result));
                    }
                }
            });
        }

        drop(tx);

        let mut results: Vec<_> = rx.into_iter().collect();
        results.sort_by_key(|(index, _)| *index);
        results
    })
}

/// Cached tracker context for AI assistants
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TrackerCache {
    /// Timestamp of last cache update
    pub updated_at: Option<String>,
    /// Backend metadata (type and URL)
    #[serde(default)]
    pub backend_metadata: Option<CachedBackendMetadata>,
    /// Default project from config
    #[serde(default)]
    pub default_project: Option<String>,
    /// List of projects with their IDs
    pub projects: Vec<CachedProject>,
    /// Custom fields per project (keyed by project shortName)
    pub project_fields: Vec<ProjectFieldsCache>,
    /// Available tags
    pub tags: Vec<CachedTag>,
    /// Available issue link types
    #[serde(default)]
    pub link_types: Vec<CachedLinkType>,
    /// Pre-built query templates for the backend
    #[serde(default)]
    pub query_templates: Vec<CachedQueryTemplate>,
    /// Assignable users per project
    #[serde(default)]
    pub project_users: Vec<ProjectUsersCache>,
    /// Workflow hints per project (state transitions)
    #[serde(default)]
    pub workflow_hints: Vec<ProjectWorkflowHints>,
    /// Recently accessed issues (LRU, max 50)
    #[serde(default)]
    pub recent_issues: Vec<CachedRecentIssue>,
    /// Knowledge base articles
    #[serde(default)]
    pub articles: Vec<CachedArticle>,
    /// Article hierarchy (parent_id -> child_ids)
    #[serde(default)]
    pub article_tree: HashMap<String, Vec<String>>,
    /// Issue counts per project and template query (populated during refresh)
    #[serde(default)]
    pub issue_counts: Vec<CachedIssueCount>,

    /// Cache directory path (for lazy loading)
    #[serde(skip)]
    pub cache_dir: Option<PathBuf>,
    /// Set of project keys that have been fully loaded
    #[serde(skip)]
    pub loaded_projects: std::collections::HashSet<String>,
    /// Tracks which shard groups have been loaded from disk
    #[serde(skip)]
    loaded_shards: LoadedShards,
}

/// Tracks which shard groups have been loaded to avoid redundant disk reads
#[derive(Debug, Default)]
struct LoadedShards {
    projects: bool,
    backend: bool,
    kb: bool,
    runtime: bool,
}

/// Cache Index for v2 (sharded) cache
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheIndexV2 {
    pub version: u32,
    pub updated_at: String,
    pub backend_metadata: CachedBackendMetadata,
    pub default_project: Option<String>,
}

/// Project shard metadata for v2 cache
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectShardMeta {
    pub id: String,
    pub short_name: String,
    pub name: String,
    pub description: Option<String>,
    pub updated_at: String,
}

/// Backend metadata for context
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedBackendMetadata {
    pub backend_type: String,
    pub base_url: String,
}

/// Pre-built query template
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedQueryTemplate {
    pub name: String,
    pub description: String,
    pub query: String,
    pub backend: String,
}

/// Cached issue link type
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedLinkType {
    pub id: String,
    pub name: String,
    pub source_to_target: Option<String>,
    pub target_to_source: Option<String>,
    pub directed: bool,
}

/// Cached user for project assignment
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedUser {
    pub id: String,
    pub login: Option<String>,
    pub display_name: String,
}

/// Cached users for a project
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectUsersCache {
    pub project_short_name: String,
    pub project_id: String,
    pub users: Vec<CachedUser>,
}

/// Recently accessed issue (for LRU cache)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedRecentIssue {
    pub id: String,
    pub id_readable: String,
    pub summary: String,
    pub project_short_name: String,
    pub state: Option<String>,
    pub last_accessed: String,
}

/// Cached issue count for a specific project + query template
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedIssueCount {
    pub project_short_name: String,
    /// Matches a `CachedQueryTemplate.name` (e.g., "unresolved", "in_progress")
    pub template_name: String,
    pub count: u64,
}

/// Cached article for knowledge base context
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedArticle {
    pub id: String,
    pub id_readable: String,
    pub summary: String,
    pub project_short_name: String,
    pub parent_id: Option<String>,
    pub has_children: bool,
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
    /// Backend-specific field ID (e.g., "customfield_10016" for Jira, field bundle ID for YouTrack)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_id: Option<String>,
    pub field_type: String,
    pub required: bool,
    /// Enum values for enum-type fields (Priority, State, Type, etc.)
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedTag {
    pub id: String,
    pub name: String,
    /// Background color hex (e.g., "#ff0000"), populated for GitHub/GitLab labels
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Description of the tag/label
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Workflow hints for a project's state fields
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectWorkflowHints {
    pub project_short_name: String,
    pub project_id: String,
    /// State fields with their workflow information
    pub state_fields: Vec<StateFieldWorkflow>,
}

/// Workflow information for a single state field
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StateFieldWorkflow {
    /// Field name (e.g., "State", "Stage")
    pub field_name: String,
    /// Available states in workflow order (sorted by ordinal)
    pub states: Vec<WorkflowState>,
    /// Valid transitions from each state
    pub transitions: Vec<StateTransition>,
}

/// A state in the workflow with metadata
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkflowState {
    /// State name
    pub name: String,
    /// Whether this is a resolved/completed state
    pub is_resolved: bool,
    /// Position in workflow (lower = earlier)
    pub ordinal: i32,
}

/// Valid transition between states
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StateTransition {
    /// Source state name
    pub from: String,
    /// Target state name
    pub to: String,
    /// Transition type: "forward", "backward", "to_resolved", "reopen"
    pub transition_type: String,
}

impl TrackerCache {
    fn load_project_metadata_shard(project_dir: &Path) -> Option<CachedProject> {
        if !project_dir.is_dir() {
            return None;
        }

        let content = fs::read_to_string(project_dir.join("meta.json")).ok()?;
        let meta = serde_json::from_str::<ProjectShardMeta>(&content).ok()?;
        Some(CachedProject {
            id: meta.id,
            short_name: meta.short_name,
            name: meta.name,
            description: meta.description,
        })
    }

    fn load_project_metadata_shards(
        project_dirs: &[PathBuf],
        worker_count: usize,
    ) -> Result<Vec<CachedProject>> {
        if project_dirs.is_empty() {
            return Ok(Vec::new());
        }

        let worker_count = worker_count.max(1).min(project_dirs.len());
        if worker_count == 1 {
            return Ok(project_dirs
                .iter()
                .filter_map(|project_dir| Self::load_project_metadata_shard(project_dir))
                .collect());
        }

        thread::scope(|scope| -> Result<Vec<CachedProject>> {
            let chunk_size = project_dirs.len().div_ceil(worker_count);
            let mut handles = Vec::with_capacity(worker_count);
            for chunk in project_dirs.chunks(chunk_size) {
                handles.push(scope.spawn(move || {
                    chunk
                        .iter()
                        .filter_map(|project_dir| Self::load_project_metadata_shard(project_dir))
                        .collect::<Vec<_>>()
                }));
            }

            let mut projects = Vec::new();
            for handle in handles {
                let mut loaded = handle
                    .join()
                    .map_err(|_| anyhow!("project metadata worker panicked"))?;
                projects.append(&mut loaded);
            }

            Ok(projects)
        })
    }

    /// Load cache from the sharded directory layout
    pub fn load(cache_dir: Option<PathBuf>) -> Result<Self> {
        let root = Self::cache_dir_path(cache_dir.clone())?;
        let index_path = root.join("index.json");
        if !index_path.exists() {
            return Ok(Self {
                cache_dir,
                ..Self::default()
            });
        }

        let index_content = fs::read_to_string(&index_path)
            .with_context(|| format!("Failed to read cache index: {}", index_path.display()))?;
        let index: CacheIndexV2 =
            serde_json::from_str(&index_content).context("Failed to parse cache index")?;

        Ok(TrackerCache {
            updated_at: Some(index.updated_at),
            backend_metadata: Some(index.backend_metadata),
            default_project: index.default_project,
            cache_dir,
            ..Self::default()
        })
    }

    /// Save cache in sharded directory layout
    pub fn save(&self, cache_dir: Option<PathBuf>) -> Result<()> {
        let root = Self::cache_dir_path(cache_dir.clone())?;
        create_dir_all_secure(&root)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&root, fs::Permissions::from_mode(0o700));
        }

        // 1. Save index.json
        let index = CacheIndexV2 {
            version: CACHE_VERSION,
            updated_at: self
                .updated_at
                .clone()
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
            backend_metadata: self
                .backend_metadata
                .clone()
                .ok_or_else(|| anyhow!("Missing backend metadata"))?,
            default_project: self.default_project.clone(),
        };
        let index_content = serde_json::to_string_pretty(&index)?;
        Self::atomic_write(&root.join("index.json"), index_content.as_bytes())?;

        // 2. Save backend shards
        let backend_dir = root.join("backend");
        create_dir_all_secure(&backend_dir)?;

        Self::atomic_write(
            &backend_dir.join("tags.json"),
            serde_json::to_string_pretty(&self.tags)?.as_bytes(),
        )?;
        Self::atomic_write(
            &backend_dir.join("query_templates.json"),
            serde_json::to_string_pretty(&self.query_templates)?.as_bytes(),
        )?;
        Self::atomic_write(
            &backend_dir.join("link_types.json"),
            serde_json::to_string_pretty(&self.link_types)?.as_bytes(),
        )?;

        // 3. Save project shards
        let projects_dir = root.join("projects");
        create_dir_all_secure(&projects_dir)?;

        for project in &self.projects {
            let project_key = &project.short_name;
            let project_dir = projects_dir.join(project_key);
            create_dir_all_secure(&project_dir)?;

            let meta = ProjectShardMeta {
                id: project.id.clone(),
                short_name: project.short_name.clone(),
                name: project.name.clone(),
                description: project.description.clone(),
                updated_at: self
                    .updated_at
                    .clone()
                    .unwrap_or_else(|| Utc::now().to_rfc3339()),
            };
            Self::atomic_write(
                &project_dir.join("meta.json"),
                serde_json::to_string_pretty(&meta)?.as_bytes(),
            )?;

            // Fields for this project
            if let Some(fields) = self.get_project_fields(&project.short_name) {
                Self::atomic_write(
                    &project_dir.join("fields.json"),
                    serde_json::to_string_pretty(&fields)?.as_bytes(),
                )?;
            }

            // Users for this project
            if let Some(users) = self.get_project_users(&project.short_name) {
                Self::atomic_write(
                    &project_dir.join("users.json"),
                    serde_json::to_string_pretty(&users)?.as_bytes(),
                )?;
            }

            // Workflow hints for this project
            if let Some(hints) = self
                .workflow_hints
                .iter()
                .find(|h| h.project_short_name == project.short_name)
            {
                Self::atomic_write(
                    &project_dir.join("workflow.json"),
                    serde_json::to_string_pretty(hints)?.as_bytes(),
                )?;
            }

            // Issue counts for this project
            let project_issue_counts: Vec<_> = self
                .issue_counts
                .iter()
                .filter(|ic| ic.project_short_name == project.short_name)
                .collect();
            if !project_issue_counts.is_empty() {
                Self::atomic_write(
                    &project_dir.join("issue_counts.json"),
                    serde_json::to_string_pretty(&project_issue_counts)?.as_bytes(),
                )?;
            }
        }

        // 4. Save kb shards
        let kb_dir = root.join("kb");
        create_dir_all_secure(&kb_dir)?;
        Self::atomic_write(
            &kb_dir.join("articles.json"),
            serde_json::to_string_pretty(&self.articles)?.as_bytes(),
        )?;
        Self::atomic_write(
            &kb_dir.join("tree.json"),
            serde_json::to_string_pretty(&self.article_tree)?.as_bytes(),
        )?;

        // 5. Save runtime shards
        let runtime_dir = root.join("runtime");
        create_dir_all_secure(&runtime_dir)?;
        Self::atomic_write(
            &runtime_dir.join("recent_issues.json"),
            serde_json::to_string_pretty(&self.recent_issues)?.as_bytes(),
        )?;

        Ok(())
    }

    /// Save only the runtime shard (recent_issues). Used for lightweight
    /// updates like recording issue access without touching other shards.
    pub fn save_runtime(&self, cache_dir: Option<PathBuf>) -> Result<()> {
        let root = Self::cache_dir_path(cache_dir)?;
        let runtime_dir = root.join("runtime");
        create_dir_all_secure(&runtime_dir)?;
        Self::atomic_write_relaxed(
            &runtime_dir.join("recent_issues.json"),
            &serde_json::to_vec(&self.recent_issues)?,
        )?;
        Ok(())
    }

    /// Load all shards eagerly
    pub fn load_all(cache_dir: Option<PathBuf>) -> Result<Self> {
        let mut cache = Self::load(cache_dir)?;
        cache.ensure_all_loaded()?;
        Ok(cache)
    }

    /// Ensure all shards are loaded
    pub fn ensure_all_loaded(&mut self) -> Result<()> {
        self.ensure_projects()?;
        self.ensure_backend_shards()?;
        self.ensure_kb_shards()?;
        self.ensure_runtime_shards()?;

        let project_keys: Vec<String> =
            self.projects.iter().map(|p| p.short_name.clone()).collect();
        for key in project_keys {
            self.ensure_project_shard(&key)?;
        }
        Ok(())
    }

    /// Ensure projects are loaded from project shards
    pub fn ensure_projects(&mut self) -> Result<()> {
        if self.loaded_shards.projects {
            return Ok(());
        }

        let root = Self::cache_dir_path(self.cache_dir.clone())?;
        let projects_dir = root.join("projects");
        if projects_dir.exists() {
            let project_dirs: Vec<PathBuf> = fs::read_dir(&projects_dir)?
                .map(|entry| entry.map(|e| e.path()))
                .collect::<std::io::Result<_>>()?;
            if project_dirs.is_empty() {
                self.loaded_shards.projects = true;
                return Ok(());
            }

            let worker_count = configured_project_metadata_worker_count(project_dirs.len());
            self.projects = Self::load_project_metadata_shards(&project_dirs, worker_count)?;
        }
        self.loaded_shards.projects = true;
        Ok(())
    }

    /// Ensure backend shards are loaded (tags, templates, link types)
    pub fn ensure_backend_shards(&mut self) -> Result<()> {
        if self.loaded_shards.backend {
            return Ok(());
        }
        self.loaded_shards.backend = true;

        let root = Self::cache_dir_path(self.cache_dir.clone())?;
        let backend_dir = root.join("backend");
        if backend_dir.exists() {
            if let Ok(content) = fs::read_to_string(backend_dir.join("tags.json"))
                && let Ok(tags) = serde_json::from_str(&content)
            {
                self.tags = tags;
            }
            if let Ok(content) = fs::read_to_string(backend_dir.join("query_templates.json"))
                && let Ok(templates) = serde_json::from_str(&content)
            {
                self.query_templates = templates;
            }
            if let Ok(content) = fs::read_to_string(backend_dir.join("link_types.json"))
                && let Ok(link_types) = serde_json::from_str(&content)
            {
                self.link_types = link_types;
            }
        }
        Ok(())
    }

    /// Ensure knowledge base shards are loaded
    pub fn ensure_kb_shards(&mut self) -> Result<()> {
        if self.loaded_shards.kb {
            return Ok(());
        }
        self.loaded_shards.kb = true;

        let root = Self::cache_dir_path(self.cache_dir.clone())?;
        let kb_dir = root.join("kb");
        if kb_dir.exists() {
            if let Ok(content) = fs::read_to_string(kb_dir.join("articles.json"))
                && let Ok(articles) = serde_json::from_str(&content)
            {
                self.articles = articles;
            }
            if let Ok(content) = fs::read_to_string(kb_dir.join("tree.json"))
                && let Ok(tree) = serde_json::from_str(&content)
            {
                self.article_tree = tree;
            }
        }
        Ok(())
    }

    /// Ensure runtime shards are loaded (recent issues)
    pub fn ensure_runtime_shards(&mut self) -> Result<()> {
        if self.loaded_shards.runtime {
            return Ok(());
        }
        self.loaded_shards.runtime = true;

        let root = Self::cache_dir_path(self.cache_dir.clone())?;
        let runtime_dir = root.join("runtime");
        if runtime_dir.exists()
            && let Ok(content) = fs::read_to_string(runtime_dir.join("recent_issues.json"))
            && let Ok(recent) = serde_json::from_str(&content)
        {
            self.recent_issues = recent;
        }
        Ok(())
    }

    /// Ensure a specific project shard is loaded
    pub fn ensure_project_shard(&mut self, project_key: &str) -> Result<()> {
        if self.loaded_projects.contains(project_key) {
            return Ok(());
        }

        let root = Self::cache_dir_path(self.cache_dir.clone())?;
        let project_dir = root.join("projects").join(project_key);
        if !project_dir.exists() {
            return Ok(());
        }

        // Resolve project_id from in-memory projects or fall back to meta.json
        let project_id = self
            .projects
            .iter()
            .find(|p| p.short_name == project_key)
            .map(|p| p.id.clone())
            .unwrap_or_else(|| {
                if let Ok(content) = fs::read_to_string(project_dir.join("meta.json"))
                    && let Ok(meta) = serde_json::from_str::<ProjectShardMeta>(&content)
                {
                    return meta.id;
                }
                "unknown".to_string()
            });

        // Load fields
        if let Ok(content) = fs::read_to_string(project_dir.join("fields.json"))
            && let Ok(fields) = serde_json::from_str::<Vec<CachedField>>(&content)
        {
            self.project_fields
                .retain(|pf| pf.project_short_name != project_key);
            self.project_fields.push(ProjectFieldsCache {
                project_short_name: project_key.to_string(),
                project_id: project_id.clone(),
                fields,
            });
        }

        // Load users
        if let Ok(content) = fs::read_to_string(project_dir.join("users.json"))
            && let Ok(users) = serde_json::from_str(&content)
        {
            self.project_users
                .retain(|pu| pu.project_short_name != project_key);
            self.project_users.push(ProjectUsersCache {
                project_short_name: project_key.to_string(),
                project_id: project_id.clone(),
                users,
            });
        }

        // Load workflow
        if let Ok(content) = fs::read_to_string(project_dir.join("workflow.json"))
            && let Ok(hints) = serde_json::from_str(&content)
        {
            self.workflow_hints
                .retain(|wh| wh.project_short_name != project_key);
            self.workflow_hints.push(hints);
        }

        // Load issue counts
        if let Ok(content) = fs::read_to_string(project_dir.join("issue_counts.json"))
            && let Ok(counts) = serde_json::from_str::<Vec<CachedIssueCount>>(&content)
        {
            self.issue_counts
                .retain(|ic| ic.project_short_name != project_key);
            self.issue_counts.extend(counts);
        }

        self.loaded_projects.insert(project_key.to_string());
        Ok(())
    }

    /// Get cache directory path.
    /// - If explicit cache_dir is provided, use it.
    /// - If in project context (.track.toml exists), use ./.tracker-cache/
    /// - Otherwise (global context), use ~/.tracker-cli/cache/
    fn cache_dir_path(cache_dir: Option<PathBuf>) -> Result<PathBuf> {
        if let Some(dir) = cache_dir {
            return Ok(dir.join(CACHE_DIR_NAME));
        }

        if crate::config::is_project_context() {
            // Project context: cache alongside .track.toml
            Ok(PathBuf::from(CACHE_DIR_NAME))
        } else {
            // Global context: cache in ~/.tracker-cli/cache/
            crate::config::global_cache_dir()
                .ok_or_else(|| anyhow!("Could not determine home directory for global cache"))
        }
    }

    /// Get the resolved cache directory path (for external use, e.g. `cache path` command)
    pub fn resolved_cache_dir() -> Result<PathBuf> {
        Self::cache_dir_path(None)
    }

    /// Atomic write helper: write temp -> fsync -> rename
    fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
        Self::atomic_write_inner(path, content, true)
    }

    /// Relaxed atomic write helper: write temp -> rename, without fsync.
    ///
    /// Used for non-critical runtime metadata on hot paths where preserving
    /// atomic replacement and file permissions matters more than durability
    /// across sudden power loss.
    fn atomic_write_relaxed(path: &Path, content: &[u8]) -> Result<()> {
        Self::atomic_write_inner(path, content, false)
    }

    fn atomic_write_inner(path: &Path, content: &[u8], durable: bool) -> Result<()> {
        let temp_path = path.with_extension("tmp");

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            create_dir_all_secure(parent)?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(
                path.parent().unwrap_or(path),
                fs::Permissions::from_mode(0o700),
            );
        }

        {
            let mut options = OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            options.mode(0o600);

            let mut file = options
                .open(&temp_path)
                .with_context(|| format!("Failed to create temp file: {}", temp_path.display()))?;

            file.write_all(content).with_context(|| {
                format!("Failed to write to temp file: {}", temp_path.display())
            })?;
            if durable {
                file.sync_all().with_context(|| {
                    format!("Failed to sync temp file: {}", temp_path.display())
                })?;
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600));
        }

        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "Failed to rename {} to {}",
                temp_path.display(),
                path.display()
            )
        })?;

        Ok(())
    }

    /// Refresh cache from tracker API
    pub fn refresh(
        client: &dyn IssueTracker,
        backend_type: &str,
        base_url: &str,
        default_project: Option<&str>,
    ) -> Result<Self> {
        let mut cache = Self {
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
            backend_metadata: Some(CachedBackendMetadata {
                backend_type: backend_type.to_string(),
                base_url: base_url.to_string(),
            }),
            default_project: default_project.map(|s| s.to_string()),
            query_templates: Self::get_query_templates(backend_type),
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

        // Determine which projects to fetch detailed data for:
        // - If default_project is set (project context), only fetch that project's details
        // - Otherwise (global context), fetch details for all projects
        let projects_for_details: Vec<&tracker_core::Project> = if let Some(dp) = default_project {
            projects
                .iter()
                .filter(|p| p.short_name.eq_ignore_ascii_case(dp))
                .collect()
        } else {
            projects.iter().collect()
        };

        // Fetch tags (instance-level, always fetch all)
        if let Ok(tags) = client.list_tags() {
            cache.tags = tags
                .into_iter()
                .map(|t| CachedTag {
                    id: t.id,
                    name: t.name,
                    color: t.color.and_then(|c| c.background),
                    description: None,
                })
                .collect();
        }

        // Fetch link types (instance-level, always fetch all)
        if let Ok(link_types) = client.list_link_types() {
            cache.link_types = link_types
                .into_iter()
                .map(|lt| CachedLinkType {
                    id: lt.id,
                    name: lt.name,
                    source_to_target: lt.source_to_target,
                    target_to_source: lt.target_to_source,
                    directed: lt.directed,
                })
                .collect();
        }

        let concurrency = cache_refresh_concurrency();

        // Fetch custom fields and build workflow hints for scoped projects
        let field_results =
            bounded_parallel_map_indexed(&projects_for_details, concurrency, |_, project| {
                let fields = client.get_project_custom_fields(&project.id).ok()?;
                let workflow_hints =
                    Self::build_workflow_hints(&project.short_name, &project.id, &fields);
                let cached_fields: Vec<CachedField> = fields
                    .into_iter()
                    .map(|f| CachedField {
                        name: f.name,
                        field_id: Some(f.id),
                        field_type: f.field_type,
                        required: f.required,
                        values: f.values,
                    })
                    .collect();

                let project_fields = ProjectFieldsCache {
                    project_short_name: project.short_name.clone(),
                    project_id: project.id.clone(),
                    fields: cached_fields,
                };

                Some((
                    project_fields,
                    (!workflow_hints.state_fields.is_empty()).then_some(workflow_hints),
                ))
            });
        for (_, (project_fields, workflow_hints)) in field_results {
            cache.project_fields.push(project_fields);
            if let Some(workflow_hints) = workflow_hints {
                cache.workflow_hints.push(workflow_hints);
            }
        }

        // Fetch project users for scoped projects
        let user_results =
            bounded_parallel_map_indexed(&projects_for_details, concurrency, |_, project| {
                let users = client.list_project_users(&project.id).ok()?;
                let cached_users: Vec<CachedUser> = users
                    .into_iter()
                    .map(|u| CachedUser {
                        id: u.id,
                        login: u.login,
                        display_name: u.display_name,
                    })
                    .collect();

                Some(ProjectUsersCache {
                    project_short_name: project.short_name.clone(),
                    project_id: project.id.clone(),
                    users: cached_users,
                })
            });
        for (_, project_users) in user_results {
            cache.project_users.push(project_users);
        }

        // Fetch issue counts for query templates
        let projects_to_count: Vec<&CachedProject> = if let Some(ref dp) = cache.default_project {
            cache
                .projects
                .iter()
                .filter(|p| p.short_name.eq_ignore_ascii_case(dp))
                .collect()
        } else {
            cache.projects.iter().collect()
        };

        struct IssueCountJob {
            project_index: usize,
            template_index: usize,
            project_short_name: String,
            template_name: String,
            query: String,
        }

        struct IssueCountResult {
            project_index: usize,
            template_index: usize,
            issue_count: CachedIssueCount,
        }

        let mut issue_count_jobs = Vec::new();
        for (project_index, project) in projects_to_count.iter().enumerate() {
            for (template_index, template) in cache.query_templates.iter().enumerate() {
                issue_count_jobs.push(IssueCountJob {
                    project_index,
                    template_index,
                    project_short_name: project.short_name.clone(),
                    template_name: template.name.clone(),
                    query: template.query.replace("{PROJECT}", &project.short_name),
                });
            }
        }

        let mut issue_count_results =
            bounded_parallel_map_indexed(&issue_count_jobs, concurrency, |_, job| {
                let count = client.get_issue_count(&job.query).ok().flatten()?;
                Some(IssueCountResult {
                    project_index: job.project_index,
                    template_index: job.template_index,
                    issue_count: CachedIssueCount {
                        project_short_name: job.project_short_name.clone(),
                        template_name: job.template_name.clone(),
                        count,
                    },
                })
            });
        issue_count_results
            .sort_by_key(|(_, result)| (result.project_index, result.template_index));
        cache.issue_counts.extend(
            issue_count_results
                .into_iter()
                .map(|(_, result)| result.issue_count),
        );
        cache.mark_refreshed_shards_loaded(
            projects_for_details.iter().map(|p| p.short_name.clone()),
        );

        Ok(cache)
    }

    fn mark_refreshed_shards_loaded<I>(&mut self, detailed_project_keys: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.loaded_shards.projects = true;
        self.loaded_shards.backend = true;
        self.loaded_shards.runtime = true;
        self.loaded_projects.extend(detailed_project_keys);
    }

    /// Refresh cache with articles from knowledge base (if available)
    pub fn refresh_with_articles(
        client: &dyn IssueTracker,
        kb_client: Option<&dyn KnowledgeBase>,
        backend_type: &str,
        base_url: &str,
        default_project: Option<&str>,
    ) -> Result<Self> {
        let mut cache = Self::refresh(client, backend_type, base_url, default_project)?;

        // Fetch articles if knowledge base client is available
        if let Some(kb) = kb_client
            && let Ok(articles) = kb.list_articles(None, 100, 0)
        {
            cache.add_articles(articles);
        }

        Ok(cache)
    }

    /// Get query templates for a backend
    fn get_query_templates(backend_type: &str) -> Vec<CachedQueryTemplate> {
        match backend_type {
            "youtrack" => vec![
                CachedQueryTemplate {
                    name: "unresolved".to_string(),
                    description: "All unresolved issues in project".to_string(),
                    query: "project: {PROJECT} #Unresolved".to_string(),
                    backend: "youtrack".to_string(),
                },
                CachedQueryTemplate {
                    name: "my_issues".to_string(),
                    description: "Issues assigned to current user".to_string(),
                    query: "project: {PROJECT} Assignee: me #Unresolved".to_string(),
                    backend: "youtrack".to_string(),
                },
                CachedQueryTemplate {
                    name: "recent".to_string(),
                    description: "Recently updated issues".to_string(),
                    query: "project: {PROJECT} updated: -7d .. Today".to_string(),
                    backend: "youtrack".to_string(),
                },
                CachedQueryTemplate {
                    name: "high_priority".to_string(),
                    description: "High priority unresolved issues".to_string(),
                    query: "project: {PROJECT} Priority: Critical,Major #Unresolved".to_string(),
                    backend: "youtrack".to_string(),
                },
                CachedQueryTemplate {
                    name: "in_progress".to_string(),
                    description: "Issues currently in progress".to_string(),
                    query: "project: {PROJECT} State: {In Progress}".to_string(),
                    backend: "youtrack".to_string(),
                },
                CachedQueryTemplate {
                    name: "bugs".to_string(),
                    description: "Bug issues".to_string(),
                    query: "project: {PROJECT} Type: Bug #Unresolved".to_string(),
                    backend: "youtrack".to_string(),
                },
            ],
            "jira" => vec![
                CachedQueryTemplate {
                    name: "unresolved".to_string(),
                    description: "All unresolved issues in project".to_string(),
                    query: "project = {PROJECT} AND resolution IS EMPTY".to_string(),
                    backend: "jira".to_string(),
                },
                CachedQueryTemplate {
                    name: "my_issues".to_string(),
                    description: "Issues assigned to current user".to_string(),
                    query: "project = {PROJECT} AND assignee = currentUser() AND resolution IS EMPTY".to_string(),
                    backend: "jira".to_string(),
                },
                CachedQueryTemplate {
                    name: "recent".to_string(),
                    description: "Recently updated issues".to_string(),
                    query: "project = {PROJECT} AND updated >= -7d".to_string(),
                    backend: "jira".to_string(),
                },
                CachedQueryTemplate {
                    name: "high_priority".to_string(),
                    description: "High priority unresolved issues".to_string(),
                    query: "project = {PROJECT} AND priority IN (Highest, High) AND resolution IS EMPTY".to_string(),
                    backend: "jira".to_string(),
                },
                CachedQueryTemplate {
                    name: "in_progress".to_string(),
                    description: "Issues currently in progress".to_string(),
                    query: "project = {PROJECT} AND status = \"In Progress\"".to_string(),
                    backend: "jira".to_string(),
                },
                CachedQueryTemplate {
                    name: "bugs".to_string(),
                    description: "Bug issues".to_string(),
                    query: "project = {PROJECT} AND issuetype = Bug AND resolution IS EMPTY".to_string(),
                    backend: "jira".to_string(),
                },
            ],
            "github" => vec![
                CachedQueryTemplate {
                    name: "unresolved".to_string(),
                    description: "All open issues in repo".to_string(),
                    query: "repo:{PROJECT} is:issue state:open".to_string(),
                    backend: "github".to_string(),
                },
                CachedQueryTemplate {
                    name: "my_issues".to_string(),
                    description: "Issues assigned to current user".to_string(),
                    query: "repo:{PROJECT} is:issue state:open assignee:@me".to_string(),
                    backend: "github".to_string(),
                },
                CachedQueryTemplate {
                    name: "recent".to_string(),
                    description: "Recently updated issues".to_string(),
                    query: "repo:{PROJECT} is:issue sort:updated-desc".to_string(),
                    backend: "github".to_string(),
                },
                CachedQueryTemplate {
                    name: "bugs".to_string(),
                    description: "Bug issues".to_string(),
                    query: "repo:{PROJECT} is:issue state:open label:bug".to_string(),
                    backend: "github".to_string(),
                },
                CachedQueryTemplate {
                    name: "enhancements".to_string(),
                    description: "Enhancement/feature request issues".to_string(),
                    query: "repo:{PROJECT} is:issue state:open label:enhancement".to_string(),
                    backend: "github".to_string(),
                },
                CachedQueryTemplate {
                    name: "no_assignee".to_string(),
                    description: "Unassigned open issues".to_string(),
                    query: "repo:{PROJECT} is:issue state:open no:assignee".to_string(),
                    backend: "github".to_string(),
                },
            ],
            "gitlab" => vec![
                CachedQueryTemplate {
                    name: "unresolved".to_string(),
                    description: "All open issues in project".to_string(),
                    query: "state=opened".to_string(),
                    backend: "gitlab".to_string(),
                },
                CachedQueryTemplate {
                    name: "my_issues".to_string(),
                    description: "Issues assigned to current user".to_string(),
                    query: "state=opened&assignee_username=@me".to_string(),
                    backend: "gitlab".to_string(),
                },
                CachedQueryTemplate {
                    name: "recent".to_string(),
                    description: "Recently updated issues".to_string(),
                    query: "state=opened&order_by=updated_at&sort=desc".to_string(),
                    backend: "gitlab".to_string(),
                },
                CachedQueryTemplate {
                    name: "bugs".to_string(),
                    description: "Bug issues".to_string(),
                    query: "state=opened&labels=bug".to_string(),
                    backend: "gitlab".to_string(),
                },
                CachedQueryTemplate {
                    name: "high_priority".to_string(),
                    description: "High priority open issues".to_string(),
                    query: "state=opened&labels=priority::high".to_string(),
                    backend: "gitlab".to_string(),
                },
                CachedQueryTemplate {
                    name: "no_assignee".to_string(),
                    description: "Unassigned open issues".to_string(),
                    query: "state=opened&assignee_id=None".to_string(),
                    backend: "gitlab".to_string(),
                },
            ],
            "linear" => vec![
                CachedQueryTemplate {
                    name: "unresolved".to_string(),
                    description: "All unresolved issues in team".to_string(),
                    query: "project: {PROJECT} #Unresolved".to_string(),
                    backend: "linear".to_string(),
                },
                CachedQueryTemplate {
                    name: "my_issues".to_string(),
                    description: "Issues assigned to current user".to_string(),
                    query: "project: {PROJECT} assignee: me #Unresolved".to_string(),
                    backend: "linear".to_string(),
                },
                CachedQueryTemplate {
                    name: "in_progress".to_string(),
                    description: "Issues currently in progress".to_string(),
                    query: "project: {PROJECT} state: started".to_string(),
                    backend: "linear".to_string(),
                },
                CachedQueryTemplate {
                    name: "bugs".to_string(),
                    description: "Bug issues".to_string(),
                    query: "project: {PROJECT} label: Bug #Unresolved".to_string(),
                    backend: "linear".to_string(),
                },
                CachedQueryTemplate {
                    name: "high_priority".to_string(),
                    description: "Urgent and high priority unresolved issues".to_string(),
                    query: "project: {PROJECT} priority: High #Unresolved".to_string(),
                    backend: "linear".to_string(),
                },
            ],
            _ => Vec::new(),
        }
    }

    /// Build workflow hints from project custom fields
    fn build_workflow_hints(
        project_short_name: &str,
        project_id: &str,
        fields: &[tracker_core::ProjectCustomField],
    ) -> ProjectWorkflowHints {
        let mut state_fields = Vec::new();

        for field in fields {
            // Only process state fields that have state_values
            if !field.state_values.is_empty() {
                // Sort states by ordinal
                let mut states: Vec<WorkflowState> = field
                    .state_values
                    .iter()
                    .map(|sv| WorkflowState {
                        name: sv.name.clone(),
                        is_resolved: sv.is_resolved,
                        ordinal: sv.ordinal,
                    })
                    .collect();
                states.sort_by_key(|s| s.ordinal);

                // Build transitions based on workflow order
                let transitions = Self::build_transitions(&states);

                state_fields.push(StateFieldWorkflow {
                    field_name: field.name.clone(),
                    states,
                    transitions,
                });
            }
        }

        ProjectWorkflowHints {
            project_short_name: project_short_name.to_string(),
            project_id: project_id.to_string(),
            state_fields,
        }
    }

    /// Build state transitions based on workflow order
    /// Heuristics:
    /// - Forward transitions: state can typically go to adjacent or nearby forward states
    /// - Resolved states: typically reachable from later workflow stages
    /// - Reopen: resolved states can go back to unresolved states
    fn build_transitions(states: &[WorkflowState]) -> Vec<StateTransition> {
        let mut transitions = Vec::new();

        for (i, from_state) in states.iter().enumerate() {
            for (j, to_state) in states.iter().enumerate() {
                if i == j {
                    continue; // Skip self-transitions
                }

                let transition_type = if from_state.is_resolved && !to_state.is_resolved {
                    // Going from resolved back to unresolved
                    "reopen"
                } else if !from_state.is_resolved && to_state.is_resolved {
                    // Going to a resolved state
                    "to_resolved"
                } else if to_state.ordinal > from_state.ordinal {
                    // Moving forward in workflow
                    "forward"
                } else {
                    // Moving backward in workflow
                    "backward"
                };

                // Include all transitions but mark their type
                // AI can use this to understand which transitions are typical vs atypical
                transitions.push(StateTransition {
                    from: from_state.name.clone(),
                    to: to_state.name.clone(),
                    transition_type: transition_type.to_string(),
                });
            }
        }

        transitions
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
            .find(|t| tracker_core::unicode_eq_ignore_case(&t.name, name))
            .map(|t| t.id.as_str())
    }

    /// Get users for a project (from cache)
    #[allow(dead_code)]
    pub fn get_project_users(&self, short_name: &str) -> Option<&[CachedUser]> {
        self.project_users
            .iter()
            .find(|pu| pu.project_short_name.eq_ignore_ascii_case(short_name))
            .map(|pu| pu.users.as_slice())
    }

    /// Record access to an issue (for LRU tracking)
    pub fn record_issue_access(&mut self, issue: &tracker_core::Issue) {
        let now = chrono::Utc::now().to_rfc3339();
        let project_short_name = issue
            .project
            .short_name
            .clone()
            .unwrap_or_else(|| "UNKNOWN".to_string());

        // Extract state from custom fields
        let state = issue.custom_fields.iter().find_map(|cf| {
            if let tracker_core::CustomField::State { value, .. } = cf {
                value.clone()
            } else {
                None
            }
        });

        let recent = CachedRecentIssue {
            id: issue.id.clone(),
            id_readable: issue.id_readable.clone(),
            summary: issue.summary.clone(),
            project_short_name,
            state,
            last_accessed: now,
        };

        // Remove existing entry if present
        self.recent_issues
            .retain(|r| r.id_readable != issue.id_readable);

        // Add at the front (most recent)
        self.recent_issues.insert(0, recent);

        // Enforce LRU limit
        if self.recent_issues.len() > MAX_RECENT_ISSUES {
            self.recent_issues.truncate(MAX_RECENT_ISSUES);
        }
    }

    /// Add articles to cache (preserving existing recent_issues)
    pub fn add_articles(&mut self, articles: Vec<tracker_core::Article>) {
        // Build article tree and cache
        self.articles.clear();
        self.article_tree.clear();

        for article in &articles {
            let project_short_name = article
                .project
                .short_name
                .clone()
                .unwrap_or_else(|| "UNKNOWN".to_string());

            let parent_id = article.parent_article.as_ref().map(|p| p.id.clone());

            // Add to article tree
            if let Some(ref pid) = parent_id {
                self.article_tree
                    .entry(pid.clone())
                    .or_default()
                    .push(article.id.clone());
            }

            self.articles.push(CachedArticle {
                id: article.id.clone(),
                id_readable: article.id_readable.clone(),
                summary: article.summary.clone(),
                project_short_name,
                parent_id,
                has_children: article.has_children,
            });
        }
    }

    /// Look up a cached issue count for a project + template name
    #[allow(dead_code)]
    pub fn get_issue_count(&self, project_short_name: &str, template_name: &str) -> Option<u64> {
        self.issue_counts
            .iter()
            .find(|c| {
                c.project_short_name
                    .eq_ignore_ascii_case(project_short_name)
                    && tracker_core::unicode_eq_ignore_case(&c.template_name, template_name)
            })
            .map(|c| c.count)
    }

    /// Get link type by name (from cache)
    #[allow(dead_code)]
    pub fn get_link_type(&self, name: &str) -> Option<&CachedLinkType> {
        self.link_types
            .iter()
            .find(|lt| tracker_core::unicode_eq_ignore_case(&lt.name, name))
    }

    /// Get recent issues (from cache)
    #[allow(dead_code)]
    pub fn get_recent_issues(&self, limit: usize) -> &[CachedRecentIssue] {
        let end = std::cmp::min(limit, self.recent_issues.len());
        &self.recent_issues[..end]
    }

    /// Get the timestamp when cache was last updated
    pub fn updated_at_datetime(&self) -> Option<DateTime<Utc>> {
        self.updated_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Get cache age as a Duration
    pub fn age(&self) -> Option<Duration> {
        self.updated_at_datetime()
            .map(|updated| Utc::now().signed_duration_since(updated))
    }

    /// Check if cache is older than the given duration
    pub fn is_stale(&self, max_age: Duration) -> bool {
        match self.age() {
            Some(age) => age > max_age,
            None => true, // No timestamp means stale
        }
    }

    /// Check if cache exists and has data
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty() && self.updated_at.is_none()
    }

    /// Format cache age as human-readable string
    pub fn age_string(&self) -> String {
        match self.age() {
            Some(age) => {
                let total_seconds = age.num_seconds();
                if total_seconds < 0 {
                    "just now".to_string()
                } else if total_seconds < 60 {
                    format!("{} seconds ago", total_seconds)
                } else if total_seconds < 3600 {
                    let minutes = total_seconds / 60;
                    if minutes == 1 {
                        "1 minute ago".to_string()
                    } else {
                        format!("{} minutes ago", minutes)
                    }
                } else if total_seconds < 86400 {
                    let hours = total_seconds / 3600;
                    if hours == 1 {
                        "1 hour ago".to_string()
                    } else {
                        format!("{} hours ago", hours)
                    }
                } else {
                    let days = total_seconds / 86400;
                    if days == 1 {
                        "1 day ago".to_string()
                    } else {
                        format!("{} days ago", days)
                    }
                }
            }
            None => "never".to_string(),
        }
    }
}

/// Parse a duration string like "1h", "30m", "1d" into a chrono Duration
///
/// Supported formats:
/// - `1h`, `2h` - hours
/// - `30m`, `15min` - minutes
/// - `1d` - days
/// - `60s` - seconds
/// - `2` - defaults to hours
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return Err(anyhow!("Empty duration string"));
    }

    // Try to parse as a number with a suffix
    let (num_str, unit) = if s.ends_with("min") {
        (&s[..s.len() - 3], "m")
    } else if s.ends_with('d') {
        (&s[..s.len() - 1], "d")
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], "h")
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], "m")
    } else if s.ends_with('s') {
        (&s[..s.len() - 1], "s")
    } else {
        // Default to hours if no suffix
        (s.as_str(), "h")
    };

    let num: i64 = num_str.trim().parse().map_err(|_| {
        anyhow!(
            "Invalid duration: '{}'. Use format like '1h', '30m', '1d'",
            s
        )
    })?;

    if num <= 0 {
        return Err(anyhow!("Duration must be positive"));
    }

    match unit {
        "d" => Ok(Duration::days(num)),
        "h" => Ok(Duration::hours(num)),
        "m" => Ok(Duration::minutes(num)),
        "s" => Ok(Duration::seconds(num)),
        _ => Err(anyhow!("Unknown duration unit: {}", unit)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn cache_save_uses_0700_permissions_for_dirs() {
        use crate::cli::Backend;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().unwrap();
        let cache_dir = dir.path().join("cache");
        let cache = TrackerCache {
            backend_metadata: Some(CachedBackendMetadata {
                backend_type: Backend::YouTrack.to_string(),
                base_url: "https://example.com".to_string(),
            }),
            ..Default::default()
        };

        cache.save(Some(cache_dir.clone())).unwrap();

        let root = TrackerCache::cache_dir_path(Some(cache_dir)).unwrap();
        let mode = fs::metadata(&root).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);

        let backend_dir = root.join("backend");
        let mode = fs::metadata(&backend_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn test_parse_duration_hours() {
        let d = parse_duration("1h").unwrap();
        assert_eq!(d.num_hours(), 1);

        let d = parse_duration("24h").unwrap();
        assert_eq!(d.num_hours(), 24);
    }

    #[test]
    fn test_parse_duration_minutes() {
        let d = parse_duration("30m").unwrap();
        assert_eq!(d.num_minutes(), 30);

        let d = parse_duration("90m").unwrap();
        assert_eq!(d.num_minutes(), 90);
    }

    #[test]
    fn test_parse_duration_days() {
        let d = parse_duration("1d").unwrap();
        assert_eq!(d.num_days(), 1);

        let d = parse_duration("7d").unwrap();
        assert_eq!(d.num_days(), 7);
    }

    #[test]
    fn test_parse_duration_seconds() {
        let d = parse_duration("60s").unwrap();
        assert_eq!(d.num_seconds(), 60);
    }

    #[test]
    fn test_parse_duration_long_forms() {
        // These use the simple suffixes
        assert_eq!(parse_duration("15min").unwrap().num_minutes(), 15);
        // day/days require space handling which is optional
        // The main use case is simple suffixes: 1h, 30m, 1d
    }

    #[test]
    fn test_parse_duration_default_to_hours() {
        let d = parse_duration("2").unwrap();
        assert_eq!(d.num_hours(), 2);
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("-1h").is_err());
        assert!(parse_duration("0h").is_err());
    }

    #[test]
    fn get_issue_count_returns_correct_value() {
        let cache = TrackerCache {
            issue_counts: vec![CachedIssueCount {
                project_short_name: "PROJ".to_string(),
                template_name: "unresolved".to_string(),
                count: 42,
            }],
            ..Default::default()
        };
        assert_eq!(cache.get_issue_count("PROJ", "unresolved"), Some(42));
        assert_eq!(cache.get_issue_count("proj", "Unresolved"), Some(42)); // case insensitive
    }

    #[test]
    fn get_issue_count_returns_none_for_unknown() {
        let cache = TrackerCache::default();
        assert_eq!(cache.get_issue_count("PROJ", "unresolved"), None);
    }

    #[test]
    fn cache_refresh_concurrency_clamps_to_conservative_range() {
        assert_eq!(
            clamp_cache_refresh_concurrency(None),
            DEFAULT_CACHE_REFRESH_CONCURRENCY
        );
        assert_eq!(clamp_cache_refresh_concurrency(Some(0)), 1);
        assert_eq!(clamp_cache_refresh_concurrency(Some(2)), 2);
        assert_eq!(
            clamp_cache_refresh_concurrency(Some(MAX_CACHE_REFRESH_CONCURRENCY + 1)),
            MAX_CACHE_REFRESH_CONCURRENCY
        );
    }

    #[test]
    fn project_metadata_worker_count_stays_sequential_below_threshold() {
        for project_count in [0, 1, 10, 50, MIN_PARALLEL_PROJECT_META_SHARDS - 1] {
            assert_eq!(
                project_metadata_worker_count(project_count, Some(MAX_CACHE_REFRESH_CONCURRENCY)),
                1
            );
        }
    }

    #[test]
    fn project_metadata_worker_count_uses_bounded_concurrency_at_threshold() {
        assert_eq!(
            project_metadata_worker_count(MIN_PARALLEL_PROJECT_META_SHARDS, None),
            DEFAULT_CACHE_REFRESH_CONCURRENCY
        );
        assert_eq!(
            project_metadata_worker_count(MIN_PARALLEL_PROJECT_META_SHARDS, Some(8)),
            8
        );
    }

    #[test]
    fn project_metadata_worker_count_clamps_configured_concurrency() {
        assert_eq!(
            project_metadata_worker_count(MIN_PARALLEL_PROJECT_META_SHARDS, Some(0)),
            MIN_CACHE_REFRESH_CONCURRENCY
        );
        assert_eq!(
            project_metadata_worker_count(
                MIN_PARALLEL_PROJECT_META_SHARDS,
                Some(MAX_CACHE_REFRESH_CONCURRENCY + 1)
            ),
            MAX_CACHE_REFRESH_CONCURRENCY
        );
    }

    #[test]
    fn bounded_parallel_map_indexed_preserves_input_order_and_skips_failures() {
        // Performance speedup is intentionally not asserted here; elapsed-time
        // checks are brittle in CI. The important contract is deterministic output.
        let items = [3, 1, 4, 1, 5, 9];

        let results =
            bounded_parallel_map_indexed(
                &items,
                4,
                |_, item| {
                    if *item == 1 { None } else { Some(item * 2) }
                },
            );

        assert_eq!(results, vec![(0, 6), (2, 8), (4, 10), (5, 18)]);
    }

    #[test]
    fn issue_counts_serde_roundtrip() {
        let mut cache = TrackerCache::default();
        cache.issue_counts.push(CachedIssueCount {
            project_short_name: "TEST".to_string(),
            template_name: "bugs".to_string(),
            count: 99,
        });
        let json = serde_json::to_string(&cache).unwrap();
        let loaded: TrackerCache = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.issue_counts.len(), 1);
        assert_eq!(loaded.issue_counts[0].count, 99);
    }

    #[test]
    fn test_cache_is_empty() {
        let cache = TrackerCache::default();
        assert!(cache.is_empty());

        let cache = TrackerCache {
            updated_at: Some("2024-01-01T00:00:00Z".to_string()),
            projects: vec![CachedProject {
                id: "1".to_string(),
                short_name: "PROJ".to_string(),
                name: "Project".to_string(),
                description: None,
            }],
            ..Default::default()
        };
        assert!(!cache.is_empty());
    }

    #[test]
    fn test_cache_age_string() {
        // Test with no timestamp
        let cache = TrackerCache::default();
        assert_eq!(cache.age_string(), "never");

        // Test with recent timestamp
        let now = Utc::now();
        let cache = TrackerCache {
            updated_at: Some(now.to_rfc3339()),
            ..Default::default()
        };
        let age_str = cache.age_string();
        assert!(age_str.contains("seconds") || age_str == "just now");
    }

    #[test]
    fn test_cache_is_stale() {
        let now = Utc::now();

        // Fresh cache (just updated)
        let cache = TrackerCache {
            updated_at: Some(now.to_rfc3339()),
            ..Default::default()
        };
        assert!(!cache.is_stale(Duration::hours(1)));

        // Stale cache (2 hours old, max age 1 hour)
        let two_hours_ago = now - Duration::hours(2);
        let cache = TrackerCache {
            updated_at: Some(two_hours_ago.to_rfc3339()),
            ..Default::default()
        };
        assert!(cache.is_stale(Duration::hours(1)));
        assert!(!cache.is_stale(Duration::hours(3)));

        // No timestamp = stale
        let cache = TrackerCache::default();
        assert!(cache.is_stale(Duration::hours(1)));
    }

    #[test]
    fn test_cache_v2_roundtrip() {
        use std::env;
        let temp_name = format!("test_cache_v2_dir_{}", Utc::now().timestamp());
        let cache_dir = env::current_dir().unwrap().join(temp_name);
        if cache_dir.exists() {
            fs::remove_dir_all(&cache_dir).unwrap();
        }

        let mut cache = TrackerCache {
            updated_at: Some(Utc::now().to_rfc3339()),
            backend_metadata: Some(CachedBackendMetadata {
                backend_type: "youtrack".to_string(),
                base_url: "https://example.com".to_string(),
            }),
            projects: vec![CachedProject {
                id: "p1".to_string(),
                short_name: "P1".to_string(),
                name: "Project 1".to_string(),
                description: Some("Description".to_string()),
            }],
            tags: vec![CachedTag {
                id: "t1".to_string(),
                name: "Tag 1".to_string(),
                color: None,
                description: None,
            }],
            ..Default::default()
        };
        cache.project_fields.push(ProjectFieldsCache {
            project_short_name: "P1".to_string(),
            project_id: "p1".to_string(),
            fields: vec![CachedField {
                name: "Field 1".to_string(),
                field_id: None,
                field_type: "string".to_string(),
                required: false,
                values: vec![],
            }],
        });

        // Save sharded cache
        cache
            .save(Some(cache_dir.clone()))
            .expect("Failed to save cache");

        // Verify layout
        let v2_root = cache_dir.join(".tracker-cache");
        assert!(v2_root.exists());
        assert!(v2_root.join("index.json").exists());
        assert!(v2_root.join("backend").join("tags.json").exists());
        assert!(
            v2_root
                .join("projects")
                .join("P1")
                .join("meta.json")
                .exists()
        );
        assert!(
            v2_root
                .join("projects")
                .join("P1")
                .join("fields.json")
                .exists()
        );

        // Load sharded cache
        let loaded = TrackerCache::load_all(Some(cache_dir.clone())).expect("Failed to load cache");

        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0].short_name, "P1");
        assert_eq!(loaded.tags.len(), 1);
        assert_eq!(loaded.project_fields.len(), 1);
        assert_eq!(loaded.project_fields[0].fields.len(), 1);
        assert_eq!(loaded.backend_metadata.unwrap().backend_type, "youtrack");

        // Clean up
        fs::remove_dir_all(&cache_dir).unwrap();
    }

    #[test]
    fn test_cache_v2_robustness() {
        use std::env;
        let temp_name = format!("test_cache_v2_robustness_{}", Utc::now().timestamp());
        let cache_dir = env::current_dir().unwrap().join(temp_name);
        if cache_dir.exists() {
            fs::remove_dir_all(&cache_dir).unwrap();
        }

        let cache = TrackerCache {
            updated_at: Some(Utc::now().to_rfc3339()),
            backend_metadata: Some(CachedBackendMetadata {
                backend_type: "youtrack".to_string(),
                base_url: "https://example.com".to_string(),
            }),
            projects: vec![
                CachedProject {
                    id: "p1".to_string(),
                    short_name: "P1".to_string(),
                    name: "Project 1".to_string(),
                    description: None,
                },
                CachedProject {
                    id: "p2".to_string(),
                    short_name: "P2".to_string(),
                    name: "Project 2".to_string(),
                    description: None,
                },
            ],
            ..Default::default()
        };

        cache.save(Some(cache_dir.clone())).unwrap();

        // Corrupt P1 meta.json
        let p1_meta = cache_dir
            .join(".tracker-cache")
            .join("projects")
            .join("P1")
            .join("meta.json");
        fs::write(p1_meta, "invalid json").unwrap();

        // Load sharded cache - should still load P2 despite P1 corruption
        let loaded = TrackerCache::load_all(Some(cache_dir.clone())).unwrap();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0].short_name, "P2");

        fs::remove_dir_all(&cache_dir).unwrap();
    }
}
