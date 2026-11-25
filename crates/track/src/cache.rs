use crate::fsutil::create_dir_all_secure;
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Once, OnceLock,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::thread;

use tracker_core::{IssueTracker, KnowledgeBase};

/// Age past which the agent briefings (`track prime`, `track context`)
/// report the cache as stale.
pub const STALE_AFTER: Duration = Duration::hours(24);

/// Directory name of a project's shard under `projects/`.
///
/// Project keys may contain path separators (GitHub `owner/repo`, GitLab
/// `group/project`); joined as-is they would nest the shard where
/// `ensure_projects` never looks. Escaping `%` too keeps names distinct.
fn project_shard_dir_name(project_key: &str) -> String {
    let mut name = String::with_capacity(project_key.len());
    for c in project_key.chars() {
        match c {
            '%' => name.push_str("%25"),
            '/' => name.push_str("%2F"),
            '\\' => name.push_str("%5C"),
            _ => name.push(c),
        }
    }
    name
}

/// The backend and instance this process works against. The cache keeps one
/// directory per backend, and a cache is only trusted for lookups when it
/// was written for the same instance.
#[derive(Debug, Clone)]
pub struct ActiveBackend {
    /// Backend name as the CLI spells it (`youtrack`, `jira`, ...)
    pub backend: String,
    /// What the credentials point at: the API endpoint plus, where a backend
    /// is bound to one repository or project, that too. In mock mode, the
    /// scenario directory.
    pub scope: String,
}

static ACTIVE_BACKEND: OnceLock<ActiveBackend> = OnceLock::new();

/// Record the backend the process works against. Called once, from `main`.
pub fn set_active_backend(backend: &str, scope: String) {
    let _ = ACTIVE_BACKEND.set(ActiveBackend {
        backend: backend.to_string(),
        scope,
    });
}

fn active_backend() -> Option<&'static ActiveBackend> {
    ACTIVE_BACKEND.get()
}

/// Shard entries a version 2 cache kept directly in the cache directory.
const LEGACY_ROOT_ENTRIES: [&str; 5] = ["backend", "projects", "kb", "runtime", "index.json"];

/// Move a version 2 cache into its backend's directory, the first time this
/// process reads or writes the cache. Working out where the cache is never
/// moves anything; this runs from [`TrackerCache::load`] and
/// [`TrackerCache::save`] only.
fn migrate_legacy_layout_once() {
    static MIGRATED: Once = Once::new();
    // Without an active backend the cache sits directly in the root, where
    // a version 2 cache already is.
    if active_backend().is_none() {
        return;
    }
    MIGRATED.call_once(|| {
        if let Ok(root) = TrackerCache::shared_root() {
            migrate_legacy_layout(&root);
        }
    });
}

/// Move a version 2 cache, which sat directly in `root`, into the directory
/// of the backend that wrote it. Best effort: a cache is only a copy of
/// tracker data, so a failed move must never fail the command.
fn migrate_legacy_layout(root: &Path) {
    let legacy_index = root.join("index.json");
    let Some(index) = TrackerCache::read_json_shard::<CacheIndexV2>(&legacy_index) else {
        return;
    };
    let backend = index.backend_metadata.backend_type.to_ascii_lowercase();
    if backend.is_empty() || !backend.chars().all(|c| c.is_ascii_alphanumeric()) {
        return;
    }
    let target = root.join(&backend);
    if target.join("index.json").exists() || create_dir_all_secure(&target).is_err() {
        return;
    }
    // The index goes last: until it has moved, the next run tries again.
    for entry in LEGACY_ROOT_ENTRIES {
        let from = root.join(entry);
        if from.exists() {
            let _ = fs::rename(&from, target.join(entry));
        }
    }
}

const CACHE_DIR_NAME: &str = ".tracker-cache";
/// Environment variable naming a directory to use as the cache.
const CACHE_DIR_ENV: &str = "TRACK_CACHE_DIR";
const CACHE_VERSION: u32 = 3;
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

/// What a fetch run on its own thread returned; a panic there goes on here.
fn join_fetch<T>(handle: thread::ScopedJoinHandle<'_, T>) -> T {
    handle
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
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
    /// The user the credentials authenticate as, when the backend reports it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_user: Option<CachedUser>,

    /// Full schemas fetched by a refresh, by project short name. Written to
    /// each project's `schema.json`; the fields above are the trimmed view.
    #[serde(skip)]
    schemas: HashMap<String, Vec<tracker_core::ProjectCustomField>>,
    /// Query templates fitted to a project's own field values, by project
    /// short name. A project without an entry uses `query_templates`.
    #[serde(skip)]
    pub project_templates: HashMap<String, Vec<CachedQueryTemplate>>,
    /// The directory passed to `load`, if any (for lazy loading)
    #[serde(skip)]
    cache_dir: Option<PathBuf>,
    /// Where the shards are, worked out from `cache_dir` on first use
    #[serde(skip)]
    dir: OnceLock<PathBuf>,
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

/// A project's schema exactly as the backend returned it, with the time it
/// was fetched. One file, so the data and its age cannot disagree.
#[derive(Debug, Serialize, Deserialize)]
struct ProjectSchemaShard<'a> {
    fetched_at: Cow<'a, str>,
    fields: Cow<'a, [tracker_core::ProjectCustomField]>,
}

const SCHEMA_SHARD: &str = "schema.json";
const FIELDS_SHARD: &str = "fields.json";
const WORKFLOW_SHARD: &str = "workflow.json";
const TEMPLATES_SHARD: &str = "templates.json";
const TAGS_SHARD: &str = "tags.json";
const RESOLVED_PROJECTS_SHARD: &str = "resolved_projects.json";

/// How hard a shard write works to survive a crash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Durability {
    /// fsync before the rename: a refresh, which rewrites the whole cache.
    Durable,
    /// No fsync: what commands write back in passing.
    Relaxed,
}

/// What the cache keeps beside a project's schema, all derived from it.
struct SchemaViews {
    /// The trimmed field list, for display.
    fields: Vec<CachedField>,
    /// The project's state fields and their states, when it has any.
    workflow: Option<ProjectWorkflowHints>,
    /// The built-in templates fitted to the schema, on backends that fit them.
    templates: Option<Vec<CachedQueryTemplate>>,
}

impl SchemaViews {
    fn of(
        backend_type: &str,
        project: &CachedProject,
        schema: &[tracker_core::ProjectCustomField],
    ) -> Self {
        let workflow = TrackerCache::build_workflow_hints(&project.short_name, &project.id, schema);
        Self {
            fields: cached_fields_from(schema),
            workflow: (!workflow.state_fields.is_empty()).then_some(workflow),
            templates: derive_templates(backend_type, schema),
        }
    }
}

/// What a project identifier resolved to, and when.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ResolvedProject {
    id: String,
    fetched_at: String,
}

/// The trimmed field view kept in `fields.json` for display.
fn cached_fields_from(fields: &[tracker_core::ProjectCustomField]) -> Vec<CachedField> {
    fields
        .iter()
        .map(|f| CachedField {
            name: f.name.clone(),
            field_id: Some(f.id.clone()),
            field_type: f.field_type.clone(),
            required: f.required,
            values: f.values.clone(),
        })
        .collect()
}

/// Names that usually mean "more urgent than normal", lowercase.
const HIGH_PRIORITY_NAMES: [&str; 8] = [
    "show-stopper",
    "blocker",
    "urgent",
    "critical",
    "highest",
    "major",
    "high",
    "p1",
];
/// Names that usually mean "being worked on", lowercase.
const IN_PROGRESS_NAMES: [&str; 7] = [
    "in progress",
    "develop",
    "in development",
    "developing",
    "doing",
    "started",
    "wip",
];

/// The query languages whose built-in templates name schema values.
#[derive(Debug, Clone, Copy)]
enum QuerySyntax {
    YouTrack,
    Jql,
}

impl QuerySyntax {
    fn of(backend: &str) -> Option<Self> {
        match backend {
            "youtrack" => Some(Self::YouTrack),
            "jira" => Some(Self::Jql),
            _ => None,
        }
    }

    /// One value, quoted as the language needs.
    fn value(self, value: &str) -> String {
        match self {
            // Braces when it holds a space.
            Self::YouTrack if value.chars().any(char::is_whitespace) => format!("{{{value}}}"),
            Self::YouTrack => value.to_string(),
            // A bare word stays bare, anything else is quoted.
            Self::Jql if !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric()) => {
                value.to_string()
            }
            Self::Jql => format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")),
        }
    }

    /// A list of values, each quoted.
    fn list(self, values: &[impl AsRef<str>]) -> String {
        let separator = match self {
            Self::YouTrack => ",",
            Self::Jql => ", ",
        };
        values
            .iter()
            .map(|value| self.value(value.as_ref()))
            .collect::<Vec<_>>()
            .join(separator)
    }

    /// `high_priority`: unresolved issues with one of `priorities`.
    fn high_priority(self, priorities: &[impl AsRef<str>]) -> String {
        match self {
            Self::YouTrack => format!(
                "project: {{PROJECT}} Priority: {} #Unresolved",
                self.list(priorities)
            ),
            Self::Jql => format!(
                "project = {{PROJECT}} AND priority IN ({}) AND resolution IS EMPTY",
                self.list(priorities)
            ),
        }
    }

    /// `in_progress`: issues in one of `states`, kept in `state_field`
    /// (JQL always calls it `status`).
    fn in_progress(self, state_field: &str, states: &[impl AsRef<str>]) -> String {
        match (self, states) {
            (Self::YouTrack, _) => format!(
                "project: {{PROJECT}} {}: {}",
                self.value(state_field),
                self.list(states)
            ),
            (Self::Jql, [state]) => format!(
                "project = {{PROJECT}} AND status = {}",
                self.value(state.as_ref())
            ),
            (Self::Jql, _) => format!(
                "project = {{PROJECT}} AND status IN ({})",
                self.list(states)
            ),
        }
    }
}

/// The backend's built-in templates fitted to one project's schema.
///
/// A built-in names values (`Priority: Critical,Major`, `State: {In Progress}`,
/// `Type: Bug`) that a project may not have, or keeps its states in a field
/// under another name. Here each such template is rebuilt from the values the
/// project does have, and left out when it has none that fit: a template
/// that is missing is better than one that silently matches nothing. The
/// built-ins come from the same builders, so a project whose schema has
/// exactly the default values gets them verbatim.
///
/// `None` for backends whose templates do not depend on the schema.
pub(crate) fn derive_templates(
    backend_type: &str,
    fields: &[tracker_core::ProjectCustomField],
) -> Option<Vec<CachedQueryTemplate>> {
    let backend = backend_type.to_ascii_lowercase();
    let syntax = QuerySyntax::of(&backend)?;
    let field_named = |name: &str| {
        fields
            .iter()
            .find(|field| tracker_core::unicode_eq_ignore_case(&field.name, name))
    };
    // Only a complete value list says what a project does not have.
    let complete = |field: &&tracker_core::ProjectCustomField| field.values_complete;
    let matching = |field: Option<&tracker_core::ProjectCustomField>, wanted: &[&str]| {
        field.filter(complete).map(|field| {
            field
                .values
                .iter()
                .filter(|value| wanted.contains(&value.to_lowercase().as_str()))
                .cloned()
                .collect::<Vec<String>>()
        })
    };
    let high = matching(field_named("Priority"), &HIGH_PRIORITY_NAMES);
    let has_bug = matching(field_named("Type"), &["bug"]).map(|bugs| !bugs.is_empty());
    // The state field, and those of its unresolved states that mean "being
    // worked on".
    let in_progress = fields
        .iter()
        .find(|field| field.field_type.to_ascii_lowercase().contains("state"))
        .filter(|field| complete(field) && !field.state_values.is_empty())
        .map(|field| {
            let states: Vec<&str> = field
                .state_values
                .iter()
                .filter(|state| {
                    !state.is_resolved
                        && IN_PROGRESS_NAMES.contains(&state.name.to_lowercase().as_str())
                })
                .map(|state| state.name.as_str())
                .collect();
            (field, states)
        });

    let templates = TrackerCache::get_query_templates(&backend)
        .into_iter()
        .filter_map(|mut template| {
            // `None`: nothing is known, so the built-in guess stands.
            // `Some(None)`: the project has no value that fits.
            let fitted: Option<Option<String>> = match template.name.as_str() {
                "high_priority" => high
                    .as_deref()
                    .map(|values| (!values.is_empty()).then(|| syntax.high_priority(values))),
                "in_progress" => in_progress.as_ref().map(|(field, states)| {
                    (!states.is_empty()).then(|| syntax.in_progress(&field.name, states))
                }),
                "bugs" => (has_bug == Some(false)).then_some(None),
                _ => None,
            };
            match fitted {
                None => Some(template),
                Some(None) => None,
                Some(Some(query)) => {
                    template.query = query;
                    Some(template)
                }
            }
        })
        .collect();
    Some(templates)
}

/// Whether an RFC 3339 timestamp is no older than `max_age`.
fn is_fresh(timestamp: &str, max_age: Duration) -> bool {
    DateTime::parse_from_rfc3339(timestamp)
        .map(|at| Utc::now() - at.with_timezone(&Utc) <= max_age)
        .unwrap_or(false)
}

/// Backend metadata for context
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedBackendMetadata {
    pub backend_type: String,
    pub base_url: String,
    /// The instance the cache was written for (see [`ActiveBackend::scope`]).
    /// Absent in caches written before lookups read through the cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
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

impl TrackerCache {
    fn load_project_metadata_shard(project_dir: &Path) -> Option<CachedProject> {
        if !project_dir.is_dir() {
            return None;
        }

        let meta: ProjectShardMeta = Self::read_json_shard(&project_dir.join("meta.json"))?;
        Some(CachedProject {
            id: meta.id,
            short_name: meta.short_name,
            name: meta.name,
            description: meta.description,
        })
    }

    /// Load cache from the sharded directory layout
    pub fn load(cache_dir: Option<PathBuf>) -> Result<Self> {
        if cache_dir.is_none() {
            migrate_legacy_layout_once();
        }
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

    /// Save cache in sharded directory layout.
    ///
    /// Writes every shard from this in-memory cache (a freshly refreshed one),
    /// and removes shards of projects it no longer lists.
    pub fn save(&self, cache_dir: Option<PathBuf>) -> Result<()> {
        use Durability::Durable;
        if cache_dir.is_none() {
            migrate_legacy_layout_once();
        }
        let root = Self::cache_dir_path(cache_dir)?;
        create_dir_all_secure(&root)?;
        let updated_at = self
            .updated_at
            .clone()
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        // 1. Save index.json
        let index = CacheIndexV2 {
            version: CACHE_VERSION,
            updated_at: updated_at.clone(),
            backend_metadata: self
                .backend_metadata
                .clone()
                .ok_or_else(|| anyhow!("Missing backend metadata"))?,
            default_project: self.default_project.clone(),
        };
        Self::write_json_shard(&root.join("index.json"), &index, Durable)?;

        // 2. Save backend shards
        let backend_dir = root.join("backend");
        create_dir_all_secure(&backend_dir)?;

        Self::write_json_shard(&backend_dir.join(TAGS_SHARD), &self.tags, Durable)?;
        Self::write_json_shard(
            &backend_dir.join("query_templates.json"),
            &self.query_templates,
            Durable,
        )?;
        Self::write_json_shard(
            &backend_dir.join("link_types.json"),
            &self.link_types,
            Durable,
        )?;
        // A refresh that could not tell who is logged in must not leave an
        // earlier answer behind.
        Self::write_or_remove_shard(
            &backend_dir.join("me.json"),
            self.current_user.as_ref(),
            Durable,
        )?;

        // 3. Save project shards
        let projects_dir = root.join("projects");
        create_dir_all_secure(&projects_dir)?;
        let shard_names: HashSet<String> = self
            .projects
            .iter()
            .map(|project| project_shard_dir_name(&project.short_name))
            .collect();
        Self::remove_stale_project_shards(&projects_dir, &shard_names)?;

        for project in &self.projects {
            let key = &project.short_name;
            let project_dir = projects_dir.join(project_shard_dir_name(key));
            create_dir_all_secure(&project_dir)?;

            let meta = ProjectShardMeta {
                id: project.id.clone(),
                short_name: key.clone(),
                name: project.name.clone(),
                description: project.description.clone(),
                updated_at: updated_at.clone(),
            };
            Self::write_json_shard(&project_dir.join("meta.json"), &meta, Durable)?;

            match self.schemas.get(key) {
                // The full schema, for lookups, and the views made from it.
                Some(schema) => Self::write_schema_shards(
                    &project_dir,
                    &ProjectSchemaShard {
                        fetched_at: Cow::Borrowed(&updated_at),
                        fields: Cow::Borrowed(schema),
                    },
                    self.get_project_fields(key).unwrap_or_default(),
                    self.workflow_hints
                        .iter()
                        .find(|hints| &hints.project_short_name == key),
                    self.project_templates.get(key).map(Vec::as_slice),
                    Durable,
                )?,
                // A project this refresh fetched no detail for keeps the
                // schema it had; only a cache put together in memory has
                // fields without one.
                None => {
                    if let Some(fields) = self.get_project_fields(key) {
                        Self::write_json_shard(&project_dir.join(FIELDS_SHARD), fields, Durable)?;
                    }
                }
            }

            // Users for this project
            if let Some(users) = self.get_project_users(key) {
                Self::write_json_shard(&project_dir.join("users.json"), &users, Durable)?;
            }

            // Issue counts for this project
            let project_issue_counts: Vec<_> = self
                .issue_counts
                .iter()
                .filter(|ic| &ic.project_short_name == key)
                .collect();
            if !project_issue_counts.is_empty() {
                Self::write_json_shard(
                    &project_dir.join("issue_counts.json"),
                    &project_issue_counts,
                    Durable,
                )?;
            }
        }

        // 4. Save kb shards
        let kb_dir = root.join("kb");
        create_dir_all_secure(&kb_dir)?;
        Self::write_json_shard(&kb_dir.join("articles.json"), &self.articles, Durable)?;
        Self::write_json_shard(&kb_dir.join("tree.json"), &self.article_tree, Durable)?;

        // 5. Save runtime shards
        let runtime_dir = root.join("runtime");
        create_dir_all_secure(&runtime_dir)?;
        Self::write_json_shard(
            &runtime_dir.join("recent_issues.json"),
            &self.recent_issues,
            Durable,
        )?;

        Ok(())
    }

    /// Write a project's schema and the views derived from it. A view the
    /// schema yields nothing for is removed, so it cannot outlive the
    /// schema it came from.
    fn write_schema_shards(
        dir: &Path,
        schema: &ProjectSchemaShard<'_>,
        fields: &[CachedField],
        workflow: Option<&ProjectWorkflowHints>,
        templates: Option<&[CachedQueryTemplate]>,
        durability: Durability,
    ) -> Result<()> {
        Self::write_json_shard(&dir.join(SCHEMA_SHARD), schema, durability)?;
        Self::write_json_shard(&dir.join(FIELDS_SHARD), fields, durability)?;
        Self::write_or_remove_shard(&dir.join(WORKFLOW_SHARD), workflow, durability)?;
        Self::write_or_remove_shard(&dir.join(TEMPLATES_SHARD), templates, durability)
    }

    /// Delete project shard directories not named in `keep`, so projects that
    /// were removed, or that belong to a previously used backend, do not
    /// reappear on the next load. Removal is best effort: a leftover shard is
    /// only stale data and must not fail the refresh.
    fn remove_stale_project_shards(projects_dir: &Path, keep: &HashSet<String>) -> Result<()> {
        for entry in fs::read_dir(projects_dir)? {
            let entry = entry?;
            let is_kept = entry
                .file_name()
                .to_str()
                .is_some_and(|name| keep.contains(name));
            if !is_kept && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
        Ok(())
    }

    /// Save only the runtime shard (recent_issues). Used for lightweight
    /// updates like recording issue access without touching other shards.
    pub fn save_runtime(&self, cache_dir: Option<PathBuf>) -> Result<()> {
        let root = Self::cache_dir_path(cache_dir)?;
        let runtime_dir = root.join("runtime");
        create_dir_all_secure(&runtime_dir)?;
        Self::write_json_shard(
            &runtime_dir.join("recent_issues.json"),
            &self.recent_issues,
            Durability::Relaxed,
        )
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

        let root = self.dir()?;
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
            self.projects = bounded_parallel_map_indexed(&project_dirs, worker_count, |_, dir| {
                Self::load_project_metadata_shard(dir)
            })
            .into_iter()
            .map(|(_, project)| project)
            .collect();
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

        let root = self.dir()?;
        let backend_dir = root.join("backend");
        if backend_dir.exists() {
            if let Some(tags) = Self::read_json_shard(&backend_dir.join(TAGS_SHARD)) {
                self.tags = tags;
            }
            if let Some(templates) =
                Self::read_json_shard(&backend_dir.join("query_templates.json"))
            {
                self.query_templates = templates;
            }
            if let Some(link_types) = Self::read_json_shard(&backend_dir.join("link_types.json")) {
                self.link_types = link_types;
            }
            self.current_user = Self::read_json_shard(&backend_dir.join("me.json"));
        }
        Ok(())
    }

    /// Ensure knowledge base shards are loaded
    pub fn ensure_kb_shards(&mut self) -> Result<()> {
        if self.loaded_shards.kb {
            return Ok(());
        }
        self.loaded_shards.kb = true;

        let root = self.dir()?;
        let kb_dir = root.join("kb");
        if kb_dir.exists() {
            if let Some(articles) = Self::read_json_shard(&kb_dir.join("articles.json")) {
                self.articles = articles;
            }
            if let Some(tree) = Self::read_json_shard(&kb_dir.join("tree.json")) {
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

        let root = self.dir()?;
        let runtime_dir = root.join("runtime");
        if runtime_dir.exists()
            && let Some(recent) = Self::read_json_shard(&runtime_dir.join("recent_issues.json"))
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

        let root = self.dir()?;
        let project_dir = root
            .join("projects")
            .join(project_shard_dir_name(project_key));
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
                Self::read_json_shard::<ProjectShardMeta>(&project_dir.join("meta.json"))
                    .map_or_else(|| "unknown".to_string(), |meta| meta.id)
            });

        // Load fields
        if let Some(fields) = Self::read_json_shard(&project_dir.join(FIELDS_SHARD)) {
            self.project_fields
                .retain(|pf| pf.project_short_name != project_key);
            self.project_fields.push(ProjectFieldsCache {
                project_short_name: project_key.to_string(),
                project_id: project_id.clone(),
                fields,
            });
        }

        // Load users
        if let Some(users) = Self::read_json_shard(&project_dir.join("users.json")) {
            self.project_users
                .retain(|pu| pu.project_short_name != project_key);
            self.project_users.push(ProjectUsersCache {
                project_short_name: project_key.to_string(),
                project_id: project_id.clone(),
                users,
            });
        }

        // Load workflow
        if let Some(hints) = Self::read_json_shard(&project_dir.join(WORKFLOW_SHARD)) {
            self.workflow_hints
                .retain(|wh| wh.project_short_name != project_key);
            self.workflow_hints.push(hints);
        }

        // Load templates fitted to this project
        if let Some(templates) = Self::read_json_shard(&project_dir.join(TEMPLATES_SHARD)) {
            self.project_templates
                .insert(project_key.to_string(), templates);
        }

        // Load issue counts
        if let Some(counts) =
            Self::read_json_shard::<Vec<CachedIssueCount>>(&project_dir.join("issue_counts.json"))
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
    /// - If `TRACK_CACHE_DIR` is set, use that directory as the cache itself.
    /// - If in project context (.track.toml exists), use ./.tracker-cache/
    /// - Otherwise (global context), use ~/.tracker-cli/cache/
    ///
    /// The last three get a subdirectory named after the active backend.
    /// Only works the path out: nothing on disk is read or moved.
    fn cache_dir_path(cache_dir: Option<PathBuf>) -> Result<PathBuf> {
        if let Some(dir) = cache_dir {
            return Ok(dir.join(CACHE_DIR_NAME));
        }
        let root = Self::shared_root()?;
        // One directory per backend, so switching backends in one place does
        // not make each refresh overwrite the other's data.
        Ok(match active_backend() {
            Some(active) => root.join(&active.backend),
            None => root,
        })
    }

    /// The directory that holds every backend's cache.
    fn shared_root() -> Result<PathBuf> {
        // An explicit override keeps runs that must not share state (the eval
        // harness, CI) away from the project and global caches.
        if let Some(dir) = std::env::var_os(CACHE_DIR_ENV).filter(|dir| !dir.is_empty()) {
            Ok(PathBuf::from(dir))
        } else if crate::config::is_project_context() {
            // Project context: cache alongside .track.toml
            Ok(PathBuf::from(CACHE_DIR_NAME))
        } else {
            // Global context: cache in ~/.tracker-cli/cache/
            crate::config::global_cache_dir()
                .ok_or_else(|| anyhow!("Could not determine home directory for global cache"))
        }
    }

    /// This cache's directory, worked out once.
    fn dir(&self) -> Result<&Path> {
        if let Some(dir) = self.dir.get() {
            return Ok(dir);
        }
        let dir = Self::cache_dir_path(self.cache_dir.clone())?;
        Ok(self.dir.get_or_init(|| dir))
    }

    /// Get the resolved cache directory path (for external use, e.g. `cache path` command)
    pub fn resolved_cache_dir() -> Result<PathBuf> {
        Self::cache_dir_path(None)
    }

    /// Write `value` to a shard file as pretty JSON, atomically. A durable
    /// write also survives a power cut; a relaxed one, for the small updates
    /// commands make in passing, skips the fsync.
    fn write_json_shard<T: Serialize + ?Sized>(
        path: &Path,
        value: &T,
        durability: Durability,
    ) -> Result<()> {
        crate::fsutil::atomic_write(
            path,
            serde_json::to_string_pretty(value)?.as_bytes(),
            durability == Durability::Durable,
        )
    }

    /// Write `value` to a shard file, or remove the file when there is none,
    /// so an earlier answer is not left behind.
    fn write_or_remove_shard<T: Serialize + ?Sized>(
        path: &Path,
        value: Option<&T>,
        durability: Durability,
    ) -> Result<()> {
        match value {
            Some(value) => Self::write_json_shard(path, value, durability),
            None => {
                let _ = fs::remove_file(path);
                Ok(())
            }
        }
    }

    /// Read a JSON shard file; `None` when it is missing or does not parse.
    fn read_json_shard<T: DeserializeOwned>(path: &Path) -> Option<T> {
        let content = fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
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
                scope: active_backend().map(|active| active.scope.clone()),
            }),
            default_project: default_project.map(|s| s.to_string()),
            query_templates: Self::get_query_templates(backend_type),
            ..Self::default()
        };

        // The instance-level fetches depend on nothing else, so they run
        // while the project list is fetched.
        let (projects, tags, current_user, link_types) = thread::scope(|scope| {
            let tags = scope.spawn(|| client.list_tags());
            let current_user = scope.spawn(|| client.current_user());
            let link_types = scope.spawn(|| client.list_link_types());
            let projects = client.list_projects();
            (
                projects,
                join_fetch(tags),
                join_fetch(current_user),
                join_fetch(link_types),
            )
        });
        let projects = projects.context("Failed to fetch projects")?;
        cache.projects = projects
            .into_iter()
            .map(|p| CachedProject {
                id: p.id,
                short_name: p.short_name,
                name: p.name,
                description: p.description,
            })
            .collect();

        // Determine which projects to fetch detailed data for:
        // - If default_project is set (project context), only fetch that project's details
        // - Otherwise (global context), fetch details for all projects
        let projects_for_details: Vec<&CachedProject> = if let Some(dp) = default_project {
            cache
                .projects
                .iter()
                .filter(|p| p.short_name.eq_ignore_ascii_case(dp))
                .collect()
        } else {
            cache.projects.iter().collect()
        };
        let detailed_project_keys: Vec<String> = projects_for_details
            .iter()
            .map(|p| p.short_name.clone())
            .collect();

        // Tags and link types are instance-level, always fetched in full.
        if let Ok(tags) = tags {
            cache.tags = tags.iter().map(cached_tag_from).collect();
        }

        // Who the credentials belong to. Optional: not every backend can say.
        if let Ok(Some(user)) = current_user {
            cache.current_user = Some(CachedUser {
                id: user.id,
                login: user.login,
                display_name: user.display_name,
            });
        }

        if let Ok(link_types) = link_types {
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

        // Fetch the schema of each scoped project, and derive what the cache
        // keeps beside it.
        let schema_results =
            bounded_parallel_map_indexed(&projects_for_details, concurrency, |_, project| {
                client.get_project_custom_fields(&project.id).ok()
            });
        for (index, schema) in schema_results {
            let project = projects_for_details[index];
            let views = SchemaViews::of(backend_type, project, &schema);
            cache.project_fields.push(ProjectFieldsCache {
                project_short_name: project.short_name.clone(),
                project_id: project.id.clone(),
                fields: views.fields,
            });
            cache.workflow_hints.extend(views.workflow);
            if let Some(templates) = views.templates {
                cache
                    .project_templates
                    .insert(project.short_name.clone(), templates);
            }
            cache.schemas.insert(project.short_name.clone(), schema);
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
            let templates = cache.templates_for(Some(&project.short_name));
            for (template_index, template) in templates.iter().enumerate() {
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
        cache.mark_refreshed_shards_loaded(detailed_project_keys);

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

    /// Get compiled built-in query templates for a backend. The ones that
    /// name schema values come from the builders [`derive_templates`] fits
    /// to each project.
    pub(crate) fn get_query_templates(backend_type: &str) -> Vec<CachedQueryTemplate> {
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
                    query: QuerySyntax::YouTrack.high_priority(&["Critical", "Major"]),
                    backend: "youtrack".to_string(),
                },
                CachedQueryTemplate {
                    name: "in_progress".to_string(),
                    description: "Issues currently in progress".to_string(),
                    query: QuerySyntax::YouTrack.in_progress("State", &["In Progress"]),
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
                    query:
                        "project = {PROJECT} AND assignee = currentUser() AND resolution IS EMPTY"
                            .to_string(),
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
                    query: QuerySyntax::Jql.high_priority(&["Highest", "High"]),
                    backend: "jira".to_string(),
                },
                CachedQueryTemplate {
                    name: "in_progress".to_string(),
                    description: "Issues currently in progress".to_string(),
                    query: QuerySyntax::Jql.in_progress("status", &["In Progress"]),
                    backend: "jira".to_string(),
                },
                CachedQueryTemplate {
                    name: "bugs".to_string(),
                    description: "Bug issues".to_string(),
                    query: "project = {PROJECT} AND issuetype = Bug AND resolution IS EMPTY"
                        .to_string(),
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

                state_fields.push(StateFieldWorkflow {
                    field_name: field.name.clone(),
                    states,
                });
            }
        }

        ProjectWorkflowHints {
            project_short_name: project_short_name.to_string(),
            project_id: project_id.to_string(),
            state_fields,
        }
    }

    /// Get fields for a project (from cache)
    pub fn get_project_fields(&self, short_name: &str) -> Option<&[CachedField]> {
        self.project_fields
            .iter()
            .find(|pf| pf.project_short_name.eq_ignore_ascii_case(short_name))
            .map(|pf| pf.fields.as_slice())
    }

    /// Get users for a project (from cache)
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

        let state = issue.common_fields().state;

        let recent = CachedRecentIssue {
            id: issue.id.clone(),
            id_readable: issue.id_readable.clone(),
            summary: issue.summary.clone(),
            project_short_name,
            state: state.cloned(),
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
    ///
    /// The briefings (`track prime`, `track context`) call a cache stale past
    /// [`STALE_AFTER`].
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

// ============================================================================
// Read-through: lookups served from, and written back to, the cache
// ============================================================================

impl TrackerCache {
    /// The cache for the active backend, if one exists and was written for
    /// the same instance. Only such a cache may answer lookups or be written
    /// to by command paths.
    pub fn open_active() -> Option<Self> {
        let cache = Self::load(None).ok()?;
        cache.matches_active().then_some(cache)
    }

    /// Whether this cache was written for the backend and instance the
    /// process is working against.
    pub fn matches_active(&self) -> bool {
        match (active_backend(), &self.backend_metadata) {
            (Some(active), Some(meta)) => {
                meta.backend_type.eq_ignore_ascii_case(&active.backend)
                    && meta.scope.as_deref() == Some(active.scope.as_str())
            }
            _ => false,
        }
    }

    /// The query templates `-T` accepts for `project`: the ones fitted to
    /// its schema when there are any, else the backend's built-ins. The
    /// project's shard must have been loaded; [`Self::templates_for_project`]
    /// loads it.
    pub fn templates_for(&self, project: Option<&str>) -> &[CachedQueryTemplate] {
        project
            .and_then(|project| {
                self.project_templates
                    .iter()
                    .find(|(key, _)| tracker_core::unicode_eq_ignore_case(key, project))
            })
            .map(|(_, templates)| templates.as_slice())
            .unwrap_or(&self.query_templates)
    }

    /// [`Self::templates_for`] a project named by key (any case) or id,
    /// after reading what that takes from disk.
    pub fn templates_for_project(&mut self, project: Option<&str>) -> &[CachedQueryTemplate] {
        let _ = self.ensure_backend_shards();
        let key = project
            .and_then(|project| self.find_project(project))
            .map(|project| project.short_name);
        if let Some(key) = &key {
            let _ = self.ensure_project_shard(key);
        }
        self.templates_for(key.as_deref().or(project))
    }

    /// A cached project by short name (any case) or id.
    pub fn find_project(&mut self, key_or_id: &str) -> Option<CachedProject> {
        self.ensure_projects().ok()?;
        self.projects
            .iter()
            .find(|p| {
                tracker_core::unicode_eq_ignore_case(&p.short_name, key_or_id) || p.id == key_or_id
            })
            .cloned()
    }

    fn project_dir(&self, short_name: &str) -> Result<PathBuf> {
        Ok(self
            .dir()?
            .join("projects")
            .join(project_shard_dir_name(short_name)))
    }

    /// A project's schema, if cached and no older than `max_age`.
    pub fn load_project_schema(
        &mut self,
        key_or_id: &str,
        max_age: Duration,
    ) -> Option<Vec<tracker_core::ProjectCustomField>> {
        let project = self.find_project(key_or_id)?;
        let dir = self.project_dir(&project.short_name).ok()?;
        let shard: ProjectSchemaShard = Self::read_json_shard(&dir.join(SCHEMA_SHARD))?;
        is_fresh(&shard.fetched_at, max_age).then(|| shard.fields.into_owned())
    }

    /// Record a schema just fetched for a project the cache already lists,
    /// with the views derived from it. A project it does not list is left
    /// alone: its shard directory is named after the backend's own key,
    /// which is not known here.
    pub fn store_project_schema(
        &mut self,
        key_or_id: &str,
        fields: &[tracker_core::ProjectCustomField],
    ) -> Result<()> {
        let Some(project) = self.find_project(key_or_id) else {
            return Ok(());
        };
        let dir = self.project_dir(&project.short_name)?;
        let backend_type = self
            .backend_metadata
            .as_ref()
            .map_or("", |meta| meta.backend_type.as_str());
        let views = SchemaViews::of(backend_type, &project, fields);
        Self::write_schema_shards(
            &dir,
            &ProjectSchemaShard {
                fetched_at: Cow::Owned(Utc::now().to_rfc3339()),
                fields: Cow::Borrowed(fields),
            },
            &views.fields,
            views.workflow.as_ref(),
            views.templates.as_deref(),
            Durability::Relaxed,
        )
    }

    /// Forget a project's schema so the next lookup fetches it again.
    pub fn invalidate_project_schema(&mut self, key_or_id: &str) {
        if let Some(project) = self.find_project(key_or_id)
            && let Ok(dir) = self.project_dir(&project.short_name)
        {
            let _ = fs::remove_file(dir.join(SCHEMA_SHARD));
        }
    }

    /// Forget every project's schema.
    pub fn invalidate_all_schemas(&mut self) {
        if self.ensure_projects().is_err() {
            return;
        }
        let Ok(projects_dir) = self.dir().map(|dir| dir.join("projects")) else {
            return;
        };
        for project in &self.projects {
            let dir = projects_dir.join(project_shard_dir_name(&project.short_name));
            let _ = fs::remove_file(dir.join(SCHEMA_SHARD));
        }
    }

    fn resolved_projects_path(&self) -> Result<PathBuf> {
        Ok(self.dir()?.join("backend").join(RESOLVED_PROJECTS_SHARD))
    }

    fn resolved_projects(&self) -> HashMap<String, ResolvedProject> {
        self.resolved_projects_path()
            .ok()
            .and_then(|path| Self::read_json_shard(&path))
            .unwrap_or_default()
    }

    /// Rewrite the remembered resolutions after `change`.
    fn update_resolved_projects(
        &self,
        change: impl FnOnce(&mut HashMap<String, ResolvedProject>),
    ) -> Result<()> {
        let path = self.resolved_projects_path()?;
        let mut map = self.resolved_projects();
        change(&mut map);
        Self::write_json_shard(&path, &map, Durability::Relaxed)
    }

    /// What `resolve_project_id` returned for `identifier`, if remembered
    /// and no older than `max_age`. Kept verbatim because a backend's
    /// resolved id need not be the id its project list reports.
    pub fn resolved_project_id(&self, identifier: &str, max_age: Duration) -> Option<String> {
        let map = self.resolved_projects();
        let entry = map.get(&tracker_core::case_key(identifier))?;
        is_fresh(&entry.fetched_at, max_age).then(|| entry.id.clone())
    }

    /// Remember what `identifier` resolved to.
    pub fn store_resolved_project_id(&self, identifier: &str, id: &str) -> Result<()> {
        self.update_resolved_projects(|map| {
            map.insert(
                tracker_core::case_key(identifier),
                ResolvedProject {
                    id: id.to_string(),
                    fetched_at: Utc::now().to_rfc3339(),
                },
            );
        })
    }

    /// Forget what `identifier` resolved to, so the next lookup asks again.
    pub fn forget_resolved_project_id(&self, identifier: &str) -> Result<()> {
        let key = tracker_core::case_key(identifier);
        if !self.resolved_projects().contains_key(&key) {
            return Ok(());
        }
        self.update_resolved_projects(|map| {
            map.remove(&key);
        })
    }

    /// After a refresh: resolve the default project once and remember the
    /// answer, so the first write that names it needs no lookup.
    pub fn remember_default_project(&self, client: &dyn IssueTracker) {
        if let Some(project) = self.default_project.as_deref()
            && let Ok(id) = client.resolve_project_id(project)
        {
            let _ = self.store_resolved_project_id(project, &id);
        }
    }

    /// Apply a change to the cached tag list and write it back.
    fn update_tags(&mut self, change: impl FnOnce(&mut Vec<CachedTag>)) -> Result<()> {
        self.ensure_backend_shards()?;
        change(&mut self.tags);
        let path = self.dir()?.join("backend").join(TAGS_SHARD);
        Self::write_json_shard(&path, &self.tags, Durability::Relaxed)
    }

    /// Replace the cached tag list with one just read from the backend.
    pub fn store_tags(&mut self, tags: &[tracker_core::IssueTag]) -> Result<()> {
        let cached: Vec<CachedTag> = tags.iter().map(cached_tag_from).collect();
        self.update_tags(|current| *current = cached)
    }

    /// Record a tag the backend just created or changed. `replacing` names
    /// the tag it was before a rename; names match in any case.
    pub fn upsert_tag(
        &mut self,
        tag: &tracker_core::IssueTag,
        replacing: Option<&str>,
    ) -> Result<()> {
        self.update_tags(|tags| {
            tags.retain(|cached| {
                !tracker_core::unicode_eq_ignore_case(&cached.name, &tag.name)
                    && !replacing
                        .is_some_and(|old| tracker_core::unicode_eq_ignore_case(&cached.name, old))
            });
            tags.push(cached_tag_from(tag));
        })
    }

    /// Forget a tag the backend just deleted (name in any case).
    pub fn remove_tag(&mut self, name: &str) -> Result<()> {
        self.update_tags(|tags| {
            tags.retain(|cached| !tracker_core::unicode_eq_ignore_case(&cached.name, name));
        })
    }

    /// Add a project, or update the name and description of one already
    /// cached. Its other shards are left as they are.
    pub fn upsert_project_meta(&mut self, project: &tracker_core::Project) -> Result<()> {
        let dir = self.project_dir(&project.short_name)?;
        create_dir_all_secure(&dir)?;
        let meta = ProjectShardMeta {
            id: project.id.clone(),
            short_name: project.short_name.clone(),
            name: project.name.clone(),
            description: project.description.clone(),
            updated_at: Utc::now().to_rfc3339(),
        };
        Self::write_json_shard(&dir.join("meta.json"), &meta, Durability::Relaxed)?;
        if self.loaded_shards.projects {
            self.projects
                .retain(|cached| cached.short_name != project.short_name);
            self.projects.push(CachedProject {
                id: meta.id,
                short_name: meta.short_name,
                name: meta.name,
                description: meta.description,
            });
        }
        Ok(())
    }
}

/// The cached form of a backend tag.
pub fn cached_tag_from(tag: &tracker_core::IssueTag) -> CachedTag {
    CachedTag {
        id: tag.id.clone(),
        name: tag.name.clone(),
        color: tag.color.as_ref().and_then(|c| c.background.clone()),
        description: None,
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

    // The checked constructors: the plain ones panic past chrono's range.
    let duration = match unit {
        "d" => Duration::try_days(num),
        "h" => Duration::try_hours(num),
        "m" => Duration::try_minutes(num),
        "s" => Duration::try_seconds(num),
        _ => return Err(anyhow!("Unknown duration unit: {}", unit)),
    };
    duration.ok_or_else(|| anyhow!("Duration too large: '{}'", s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_past_the_representable_range_is_an_error_not_a_panic() {
        for huge in [
            "99999999999999d",
            "9999999999999999h",
            "9223372036854775807m",
            "9223372036854775807s",
        ] {
            assert!(parse_duration(huge).is_err(), "{huge}");
        }
        assert_eq!(parse_duration("2d").unwrap(), Duration::days(2));
    }

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
                scope: None,
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
                scope: None,
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
                scope: None,
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

    fn cache_with_projects(short_names: &[&str]) -> TrackerCache {
        TrackerCache {
            updated_at: Some(Utc::now().to_rfc3339()),
            backend_metadata: Some(CachedBackendMetadata {
                backend_type: "github".to_string(),
                base_url: "https://api.github.com".to_string(),
                scope: None,
            }),
            projects: short_names
                .iter()
                .map(|short_name| CachedProject {
                    id: format!("id-{short_name}"),
                    short_name: short_name.to_string(),
                    name: short_name.to_string(),
                    description: None,
                })
                .collect(),
            project_fields: short_names
                .iter()
                .map(|short_name| ProjectFieldsCache {
                    project_short_name: short_name.to_string(),
                    project_id: format!("id-{short_name}"),
                    fields: vec![CachedField {
                        name: "Priority".to_string(),
                        field_id: None,
                        field_type: "enum".to_string(),
                        required: false,
                        values: vec!["High".to_string()],
                    }],
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn project_keys_with_path_separators_survive_reload() {
        // GitHub (owner/repo) and GitLab (group/project) keys contain '/'.
        let dir = tempfile::TempDir::new().unwrap();
        let cache_dir = Some(dir.path().to_path_buf());
        cache_with_projects(&["acme/widgets", "group/sub/project"])
            .save(cache_dir.clone())
            .unwrap();

        let loaded = TrackerCache::load_all(cache_dir).unwrap();
        let mut names: Vec<&str> = loaded
            .projects
            .iter()
            .map(|p| p.short_name.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, vec!["acme/widgets", "group/sub/project"]);
        assert!(loaded.get_project_fields("acme/widgets").is_some());
        assert!(loaded.get_project_fields("group/sub/project").is_some());
    }

    #[test]
    fn project_shard_dir_name_is_a_single_distinct_component() {
        assert_eq!(project_shard_dir_name("DEMO"), "DEMO");
        assert_eq!(project_shard_dir_name("acme/widgets"), "acme%2Fwidgets");
        assert_eq!(project_shard_dir_name("a\\b"), "a%5Cb");
        assert_ne!(
            project_shard_dir_name("a/b"),
            project_shard_dir_name("a%2Fb")
        );
    }

    #[test]
    fn save_removes_shards_of_projects_no_longer_listed() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache_dir = Some(dir.path().to_path_buf());
        cache_with_projects(&["OLD", "KEPT"])
            .save(cache_dir.clone())
            .unwrap();

        // A later refresh (e.g. after a project was deleted, or after switching
        // backends) no longer lists OLD.
        cache_with_projects(&["KEPT"])
            .save(cache_dir.clone())
            .unwrap();

        let loaded = TrackerCache::load_all(cache_dir).unwrap();
        let names: Vec<&str> = loaded
            .projects
            .iter()
            .map(|p| p.short_name.as_str())
            .collect();
        assert_eq!(names, vec!["KEPT"]);
        assert!(loaded.get_project_fields("OLD").is_none());
    }

    #[test]
    fn atomic_write_temp_path_is_unique_per_process() {
        let temp =
            crate::fsutil::atomic_write_temp_path(Path::new("cache").join("index.json").as_path());
        assert_eq!(temp.parent(), Some(Path::new("cache")));
        assert_eq!(
            temp.file_name().unwrap().to_str().unwrap(),
            format!("index.json.{}.tmp", std::process::id())
        );
    }

    fn schema_field(
        name: &str,
        field_type: &str,
        values: &[&str],
        resolved: &[&str],
    ) -> tracker_core::ProjectCustomField {
        tracker_core::ProjectCustomField {
            id: name.to_lowercase(),
            name: name.to_string(),
            field_type: field_type.to_string(),
            required: false,
            values: values.iter().map(|v| v.to_string()).collect(),
            state_values: if field_type.contains("state") {
                values
                    .iter()
                    .enumerate()
                    .map(|(ordinal, value)| tracker_core::StateValueInfo {
                        name: value.to_string(),
                        is_resolved: resolved.contains(value),
                        ordinal: ordinal as i32,
                    })
                    .collect()
            } else {
                vec![]
            },
            values_complete: !values.is_empty(),
        }
    }

    fn template_query(templates: &[CachedQueryTemplate], name: &str) -> Option<String> {
        templates
            .iter()
            .find(|template| template.name == name)
            .map(|template| template.query.clone())
    }

    #[test]
    fn derived_templates_use_the_projects_own_names_and_values() {
        let schema = vec![
            schema_field(
                "Stage",
                "state[1]",
                &["Backlog", "In Development", "Shipped"],
                &["Shipped"],
            ),
            schema_field(
                "Priority",
                "enum[1]",
                &["Show-stopper", "Normal", "Minor"],
                &[],
            ),
            schema_field("Type", "enum[1]", &["Bug", "Feature"], &[]),
        ];

        let templates = derive_templates("youtrack", &schema).unwrap();

        assert_eq!(
            template_query(&templates, "in_progress").as_deref(),
            Some("project: {PROJECT} Stage: {In Development}")
        );
        assert_eq!(
            template_query(&templates, "high_priority").as_deref(),
            Some("project: {PROJECT} Priority: Show-stopper #Unresolved")
        );
        assert!(template_query(&templates, "bugs").is_some());
        // Templates that do not depend on the schema are untouched.
        assert_eq!(
            template_query(&templates, "unresolved").as_deref(),
            Some("project: {PROJECT} #Unresolved")
        );
    }

    #[test]
    fn a_template_with_no_fitting_value_is_left_out() {
        let schema = vec![
            schema_field("State", "state[1]", &["Open", "Closed"], &["Closed"]),
            schema_field("Priority", "enum[1]", &["Low", "Normal"], &[]),
            schema_field("Type", "enum[1]", &["Task", "Feature"], &[]),
        ];

        let templates = derive_templates("youtrack", &schema).unwrap();

        assert_eq!(template_query(&templates, "in_progress"), None);
        assert_eq!(template_query(&templates, "high_priority"), None);
        assert_eq!(template_query(&templates, "bugs"), None);
        assert!(template_query(&templates, "unresolved").is_some());
    }

    #[test]
    fn standard_projects_keep_the_built_in_queries_verbatim() {
        // Cached counts are matched by query text, so a project with the
        // usual values must derive exactly what the built-ins say.
        let schema = vec![
            schema_field(
                "State",
                "state[1]",
                &["Open", "In Progress", "Done"],
                &["Done"],
            ),
            schema_field("Priority", "enum[1]", &["Critical", "Major", "Normal"], &[]),
            schema_field("Type", "enum[1]", &["Bug", "Task"], &[]),
        ];

        let jira_schema = vec![
            schema_field(
                "Status",
                "state[1]",
                &["To Do", "In Progress", "Done"],
                &["Done"],
            ),
            schema_field("Priority", "enum[1]", &["Highest", "High", "Medium"], &[]),
        ];

        for (backend, schema) in [("youtrack", schema), ("jira", jira_schema)] {
            let derived = derive_templates(backend, &schema).unwrap();
            let built_in = TrackerCache::get_query_templates(backend);
            assert_eq!(derived.len(), built_in.len(), "{backend}");
            for template in &built_in {
                assert_eq!(
                    template_query(&derived, &template.name).as_deref(),
                    Some(template.query.as_str()),
                    "{backend} {}",
                    template.name
                );
            }
        }
    }

    #[test]
    fn guessed_value_lists_do_not_drive_derivation() {
        // A list the backend could not confirm says nothing about what the
        // project lacks, so the built-in stays.
        let mut priority = schema_field("Priority", "enum[1]", &["Low", "Normal"], &[]);
        priority.values_complete = false;

        let templates = derive_templates("jira", &[priority]).unwrap();

        assert_eq!(
            template_query(&templates, "high_priority").as_deref(),
            Some("project = {PROJECT} AND priority IN (Highest, High) AND resolution IS EMPTY")
        );
    }

    #[test]
    fn jira_templates_quote_multi_word_values() {
        let schema = vec![
            schema_field(
                "Status",
                "state[1]",
                &["To Do", "In Progress", "Done"],
                &["Done"],
            ),
            schema_field("Priority", "enum[1]", &["Blocker", "High", "Low"], &[]),
        ];

        let templates = derive_templates("jira", &schema).unwrap();

        // One state reads as the built-in does.
        assert_eq!(
            template_query(&templates, "in_progress").as_deref(),
            Some("project = {PROJECT} AND status = \"In Progress\"")
        );
        assert_eq!(
            template_query(&templates, "high_priority").as_deref(),
            Some("project = {PROJECT} AND priority IN (Blocker, High) AND resolution IS EMPTY")
        );

        let several = vec![schema_field(
            "Status",
            "state[1]",
            &["To Do", "In Progress", "Doing", "Done"],
            &["Done"],
        )];
        let templates = derive_templates("jira", &several).unwrap();
        assert_eq!(
            template_query(&templates, "in_progress").as_deref(),
            Some("project = {PROJECT} AND status IN (\"In Progress\", Doing)")
        );
    }

    #[test]
    fn backends_without_schema_dependent_templates_derive_nothing() {
        assert!(derive_templates("github", &[]).is_none());
        assert!(derive_templates("gitlab", &[]).is_none());
        assert!(derive_templates("linear", &[]).is_none());
    }

    #[test]
    fn schema_round_trips_through_its_shard_and_expires() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = cache_with_projects(&["DEMO"]);
        cache.save(Some(temp.path().to_path_buf())).unwrap();
        let mut cache = TrackerCache::load(Some(temp.path().to_path_buf())).unwrap();

        let schema = vec![schema_field(
            "Stage",
            "state[1]",
            &["Open", "Done"],
            &["Done"],
        )];
        cache.store_project_schema("demo", &schema).unwrap();

        let loaded = cache
            .load_project_schema("DEMO", Duration::hours(1))
            .expect("fresh schema");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].state_values.len(), 2);
        assert!(loaded[0].state_values[1].is_resolved);
        assert!(loaded[0].values_complete);
        // Nothing is fresh enough for a negative age.
        assert!(
            cache
                .load_project_schema("DEMO", Duration::seconds(-1))
                .is_none()
        );
        // A project the cache does not list is not given a shard.
        cache.store_project_schema("NOPE", &schema).unwrap();
        assert!(
            cache
                .load_project_schema("NOPE", Duration::hours(1))
                .is_none()
        );

        cache.invalidate_project_schema("DEMO");
        assert!(
            cache
                .load_project_schema("DEMO", Duration::hours(1))
                .is_none()
        );
    }

    #[test]
    fn resolved_project_ids_are_remembered_case_insensitively() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = cache_with_projects(&["DEMO"]);
        cache.save(Some(temp.path().to_path_buf())).unwrap();
        let cache = TrackerCache::load(Some(temp.path().to_path_buf())).unwrap();

        assert_eq!(cache.resolved_project_id("DEMO", Duration::hours(1)), None);
        cache.store_resolved_project_id("Demo", "0-1").unwrap();
        assert_eq!(
            cache
                .resolved_project_id("DEMO", Duration::hours(1))
                .as_deref(),
            Some("0-1")
        );
        assert_eq!(
            cache.resolved_project_id("DEMO", Duration::seconds(-1)),
            None
        );

        cache.forget_resolved_project_id("demo").unwrap();
        assert_eq!(cache.resolved_project_id("DEMO", Duration::hours(1)), None);
        // Forgetting what was never remembered writes nothing.
        cache.forget_resolved_project_id("OTHER").unwrap();
    }

    fn youtrack_project(short_name: &str, id: &str) -> CachedProject {
        CachedProject {
            id: id.to_string(),
            short_name: short_name.to_string(),
            name: short_name.to_string(),
            description: None,
        }
    }

    /// A cache as `refresh` leaves it, with `schema` fetched for one project.
    fn refreshed_cache(project: &CachedProject, schema: &[ProjectCustomField]) -> TrackerCache {
        let mut cache = TrackerCache {
            updated_at: Some(Utc::now().to_rfc3339()),
            backend_metadata: Some(CachedBackendMetadata {
                backend_type: "youtrack".to_string(),
                base_url: "https://example.com".to_string(),
                scope: None,
            }),
            projects: vec![project.clone()],
            query_templates: TrackerCache::get_query_templates("youtrack"),
            ..Default::default()
        };
        let views = SchemaViews::of("youtrack", project, schema);
        cache.project_fields.push(ProjectFieldsCache {
            project_short_name: project.short_name.clone(),
            project_id: project.id.clone(),
            fields: views.fields,
        });
        cache.workflow_hints.extend(views.workflow);
        if let Some(templates) = views.templates {
            cache
                .project_templates
                .insert(project.short_name.clone(), templates);
        }
        cache
            .schemas
            .insert(project.short_name.clone(), schema.to_vec());
        cache
    }

    fn project_shard(root: &Path, name: &str) -> Option<String> {
        fs::read_to_string(root.join(".tracker-cache/projects/DEMO").join(name)).ok()
    }

    use tracker_core::ProjectCustomField;

    #[test]
    fn refresh_and_lookups_write_the_same_schema_shards() {
        let project = youtrack_project("DEMO", "0-1");
        let schema = vec![
            schema_field(
                "Stage",
                "state[1]",
                &["Open", "In Development", "Done"],
                &["Done"],
            ),
            schema_field("Priority", "enum[1]", &["Critical", "Normal"], &[]),
        ];

        let saved = tempfile::TempDir::new().unwrap();
        refreshed_cache(&project, &schema)
            .save(Some(saved.path().to_path_buf()))
            .unwrap();

        let stored = tempfile::TempDir::new().unwrap();
        refreshed_cache(&project, &[])
            .save(Some(stored.path().to_path_buf()))
            .unwrap();
        let mut cache = TrackerCache::load(Some(stored.path().to_path_buf())).unwrap();
        cache.store_project_schema("0-1", &schema).unwrap();

        for shard in [FIELDS_SHARD, WORKFLOW_SHARD, TEMPLATES_SHARD] {
            let written = project_shard(saved.path(), shard);
            assert!(written.is_some(), "{shard}");
            assert_eq!(written, project_shard(stored.path(), shard), "{shard}");
        }
        let schema_of = |root: &Path| {
            let shard: serde_json::Value =
                serde_json::from_str(&project_shard(root, SCHEMA_SHARD).unwrap()).unwrap();
            shard["fields"].clone()
        };
        assert_eq!(schema_of(saved.path()), schema_of(stored.path()));
    }

    #[test]
    fn a_schema_without_states_leaves_no_workflow_behind() {
        let project = youtrack_project("DEMO", "0-1");
        let with_states = vec![schema_field(
            "State",
            "state[1]",
            &["Open", "Done"],
            &["Done"],
        )];
        let without = vec![schema_field("Priority", "enum[1]", &["Normal"], &[])];

        // A refresh after the state field went away.
        let dir = tempfile::TempDir::new().unwrap();
        let root = Some(dir.path().to_path_buf());
        refreshed_cache(&project, &with_states)
            .save(root.clone())
            .unwrap();
        assert!(project_shard(dir.path(), WORKFLOW_SHARD).is_some());
        refreshed_cache(&project, &without)
            .save(root.clone())
            .unwrap();
        assert!(project_shard(dir.path(), WORKFLOW_SHARD).is_none());

        // A lookup that fetched the schema.
        refreshed_cache(&project, &with_states)
            .save(root.clone())
            .unwrap();
        let mut cache = TrackerCache::load(root).unwrap();
        cache.store_project_schema("DEMO", &without).unwrap();
        assert!(project_shard(dir.path(), WORKFLOW_SHARD).is_none());
    }

    #[test]
    fn templates_for_a_project_named_by_its_id_are_its_own() {
        let project = youtrack_project("DEMO", "0-1");
        let schema = vec![schema_field(
            "Stage",
            "state[1]",
            &["Open", "In Development", "Done"],
            &["Done"],
        )];
        let dir = tempfile::TempDir::new().unwrap();
        let root = Some(dir.path().to_path_buf());
        refreshed_cache(&project, &schema)
            .save(root.clone())
            .unwrap();
        let fitted = Some("project: {PROJECT} Stage: {In Development}".to_string());

        for name in ["0-1", "demo", "DEMO"] {
            let mut cache = TrackerCache::load(root.clone()).unwrap();
            let templates = cache.templates_for_project(Some(name));
            assert_eq!(template_query(templates, "in_progress"), fitted, "{name}");
        }
        let mut cache = TrackerCache::load(root).unwrap();
        assert_eq!(
            template_query(cache.templates_for_project(None), "in_progress").as_deref(),
            Some("project: {PROJECT} State: {In Progress}")
        );
    }

    #[test]
    fn tag_changes_match_names_in_any_case() {
        let tag = |id: &str, name: &str| tracker_core::IssueTag {
            id: id.to_string(),
            name: name.to_string(),
            color: None,
            issues_count: None,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let root = Some(dir.path().to_path_buf());
        let mut cache = cache_with_projects(&[]);
        cache.tags = vec![
            cached_tag_from(&tag("1", "Bug")),
            cached_tag_from(&tag("2", "Old")),
        ];
        cache.save(root.clone()).unwrap();

        let mut cache = TrackerCache::load(root.clone()).unwrap();
        cache.upsert_tag(&tag("9", "bug"), None).unwrap();
        cache.upsert_tag(&tag("3", "New"), Some("old")).unwrap();
        let names = |cache: &TrackerCache| -> Vec<(String, String)> {
            let mut names: Vec<_> = cache
                .tags
                .iter()
                .map(|t| (t.id.clone(), t.name.clone()))
                .collect();
            names.sort();
            names
        };
        let expected = vec![
            ("3".to_string(), "New".to_string()),
            ("9".to_string(), "bug".to_string()),
        ];
        assert_eq!(names(&cache), expected);

        let mut reloaded = TrackerCache::load(root.clone()).unwrap();
        reloaded.ensure_backend_shards().unwrap();
        assert_eq!(names(&reloaded), expected);

        reloaded.remove_tag("NEW").unwrap();
        let mut reloaded = TrackerCache::load(root).unwrap();
        reloaded.ensure_backend_shards().unwrap();
        assert_eq!(names(&reloaded), vec![("9".to_string(), "bug".to_string())]);
    }
}
