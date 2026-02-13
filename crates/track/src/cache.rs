use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracker_core::{IssueTracker, KnowledgeBase};

const CACHE_FILE_NAME: &str = ".tracker-cache.json";
const MAX_RECENT_ISSUES: usize = 50;

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

        // Fetch custom fields for each project and build workflow hints
        for project in &projects {
            if let Ok(fields) = client.get_project_custom_fields(&project.id) {
                let cached_fields: Vec<CachedField> = fields
                    .iter()
                    .map(|f| CachedField {
                        name: f.name.clone(),
                        field_type: f.field_type.clone(),
                        required: f.required,
                        values: f.values.clone(),
                    })
                    .collect();

                cache.project_fields.push(ProjectFieldsCache {
                    project_short_name: project.short_name.clone(),
                    project_id: project.id.clone(),
                    fields: cached_fields,
                });

                // Build workflow hints for state fields
                let workflow_hints =
                    Self::build_workflow_hints(&project.short_name, &project.id, &fields);
                if !workflow_hints.state_fields.is_empty() {
                    cache.workflow_hints.push(workflow_hints);
                }
            }
        }

        // Fetch tags
        if let Ok(tags) = client.list_tags() {
            cache.tags = tags
                .iter()
                .map(|t| CachedTag {
                    id: t.id.clone(),
                    name: t.name.clone(),
                    color: t.color.as_ref().and_then(|c| c.background.clone()),
                    description: None,
                })
                .collect();
        }

        // Fetch link types
        if let Ok(link_types) = client.list_link_types() {
            cache.link_types = link_types
                .iter()
                .map(|lt| CachedLinkType {
                    id: lt.id.clone(),
                    name: lt.name.clone(),
                    source_to_target: lt.source_to_target.clone(),
                    target_to_source: lt.target_to_source.clone(),
                    directed: lt.directed,
                })
                .collect();
        }

        // Fetch project users
        for project in &projects {
            if let Ok(users) = client.list_project_users(&project.id) {
                let cached_users: Vec<CachedUser> = users
                    .iter()
                    .map(|u| CachedUser {
                        id: u.id.clone(),
                        login: u.login.clone(),
                        display_name: u.display_name.clone(),
                    })
                    .collect();

                cache.project_users.push(ProjectUsersCache {
                    project_short_name: project.short_name.clone(),
                    project_id: project.id.clone(),
                    users: cached_users,
                });
            }
        }

        Ok(cache)
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
        if let Some(kb) = kb_client {
            if let Ok(articles) = kb.list_articles(None, 100, 0) {
                cache.add_articles(articles);
            }
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
            .find(|t| t.name.eq_ignore_ascii_case(name))
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

    /// Get link type by name (from cache)
    #[allow(dead_code)]
    pub fn get_link_type(&self, name: &str) -> Option<&CachedLinkType> {
        self.link_types
            .iter()
            .find(|lt| lt.name.eq_ignore_ascii_case(name))
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
}
