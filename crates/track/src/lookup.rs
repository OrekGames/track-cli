//! Project lookups that read through the local cache.
//!
//! Commands need a project's id and schema to build and check a write. Both
//! change rarely, so [`Lookup`] answers from the cache when it can and
//! writes back what it had to fetch.
//!
//! The cache only ever shortens the happy path. A caller that gets a "no"
//! out of cached data (unknown field, invalid value) asks again with the
//! `*_live` methods before it reports an error, so stale data cannot reject
//! something the tracker would accept.

use crate::cache::TrackerCache;
use anyhow::{Context, Result};
use chrono::Duration;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracker_core::{IssueTracker, ProjectCustomField};

/// How long a cached schema or project id answers lookups.
const DEFAULT_TTL_HOURS: i64 = 24;
/// Overrides the lifetime above, in `cache refresh --if-stale` syntax (`30m`, `7d`).
const TTL_ENV: &str = "TRACK_SCHEMA_TTL";
/// Set to anything but `0` to answer every lookup from the backend.
const NO_CACHE_ENV: &str = "TRACK_NO_CACHE";

static CACHE_DISABLED: AtomicBool = AtomicBool::new(false);

/// Turn the read-through cache off for this process (`--no-cache`).
pub fn disable_cache() {
    CACHE_DISABLED.store(true, Ordering::Relaxed);
}

fn cache_disabled() -> bool {
    CACHE_DISABLED.load(Ordering::Relaxed)
        || std::env::var(NO_CACHE_ENV).is_ok_and(|value| !value.is_empty() && value != "0")
}

fn ttl() -> Duration {
    std::env::var(TTL_ENV)
        .ok()
        .and_then(|value| crate::cache::parse_duration(&value).ok())
        .unwrap_or_else(|| Duration::hours(DEFAULT_TTL_HOURS))
}

/// A project schema shared between the steps of one command.
pub type Schema = Rc<[ProjectCustomField]>;

/// A schema and whether the backend supplied it during this run.
#[derive(Clone)]
struct KnownSchema {
    fields: Schema,
    /// False when it came from the cache and may be out of date.
    live: bool,
}

/// The project an issue belongs to, as far as could be told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueProject {
    /// Project id to pass to schema lookups.
    pub id: String,
    /// True when it was read off the issue id's prefix instead of asked of
    /// the backend. An issue moved between projects keeps answering to its
    /// old id, so this can be the wrong project.
    pub derived: bool,
}

pub struct Lookup<'a> {
    client: &'a dyn IssueTracker,
    /// `None` when there is no cache for this backend and instance, or the
    /// cache is switched off.
    cache: RefCell<Option<TrackerCache>>,
    ttl: Duration,
    schemas: RefCell<HashMap<String, KnownSchema>>,
    issue_projects: RefCell<HashMap<String, String>>,
}

impl<'a> Lookup<'a> {
    pub fn new(client: &'a dyn IssueTracker) -> Self {
        let cache = if cache_disabled() {
            None
        } else {
            TrackerCache::open_active()
        };
        Self {
            client,
            cache: RefCell::new(cache),
            ttl: ttl(),
            schemas: RefCell::new(HashMap::new()),
            issue_projects: RefCell::new(HashMap::new()),
        }
    }

    pub fn client(&self) -> &'a dyn IssueTracker {
        self.client
    }

    /// Run `f` on the cache, if there is one to use.
    fn with_cache<T>(&self, f: impl FnOnce(&mut TrackerCache) -> Option<T>) -> Option<T> {
        self.cache.borrow_mut().as_mut().and_then(f)
    }

    /// Resolve a project key, name or id to the backend's project id.
    pub fn project_id(&self, identifier: &str) -> Result<String> {
        if let Some(id) = self.with_cache(|cache| cache.resolved_project_id(identifier, self.ttl)) {
            return Ok(id);
        }
        let id = self
            .client
            .resolve_project_id(identifier)
            .with_context(|| format!("Failed to resolve project '{}'", identifier))?;
        self.with_cache(|cache| cache.store_resolved_project_id(identifier, &id).ok());
        Ok(id)
    }

    /// A project's schema: from this run, then the cache, then the backend.
    pub fn schema(&self, project_id: &str) -> Result<Schema> {
        if let Some(schema) = self.schema_if_free(project_id) {
            return Ok(schema);
        }
        self.schema_live(project_id)
    }

    /// A project's schema if it costs no request: already known this run,
    /// cached, or the same for every project on this backend.
    pub fn schema_if_free(&self, project_id: &str) -> Option<Schema> {
        if let Some(known) = self.schemas.borrow().get(project_id) {
            return Some(known.fields.clone());
        }
        if self.client.schema_is_project_independent() {
            return self.schema_live(project_id).ok();
        }
        let fields = self.with_cache(|cache| cache.load_project_schema(project_id, self.ttl))?;
        let fields: Schema = fields.into();
        self.schemas.borrow_mut().insert(
            project_id.to_string(),
            KnownSchema {
                fields: fields.clone(),
                live: false,
            },
        );
        Some(fields)
    }

    /// Fetch a project's schema from the backend and remember it.
    pub fn schema_live(&self, project_id: &str) -> Result<Schema> {
        let fields = self
            .client
            .get_project_custom_fields(project_id)
            .with_context(|| {
                format!("Failed to fetch custom fields for project '{}'", project_id)
            })?;
        self.with_cache(|cache| cache.store_project_schema(project_id, &fields).ok());
        let fields: Schema = fields.into();
        self.schemas.borrow_mut().insert(
            project_id.to_string(),
            KnownSchema {
                fields: fields.clone(),
                live: true,
            },
        );
        Ok(fields)
    }

    /// Whether the schema this run holds for `project_id` came from the
    /// cache, so a rejection based on it deserves a second, live opinion.
    pub fn schema_may_be_stale(&self, project_id: &str) -> bool {
        self.schemas
            .borrow()
            .get(project_id)
            .is_some_and(|known| !known.live)
    }

    /// The project of `issue_id`, asking the backend only when the cache
    /// cannot tell.
    pub fn issue_project(&self, issue_id: &str) -> Result<IssueProject> {
        if let Some(project) = self.issue_project_if_free(issue_id) {
            return Ok(project);
        }
        self.issue_project_live(issue_id)
    }

    /// The project of `issue_id` if it costs no request.
    pub fn issue_project_if_free(&self, issue_id: &str) -> Option<IssueProject> {
        if let Some(id) = self.issue_projects.borrow().get(issue_id) {
            return Some(IssueProject {
                id: id.clone(),
                derived: false,
            });
        }
        if self.client.schema_is_project_independent() {
            // The schema lookup ignores the project on these backends.
            return Some(IssueProject {
                id: String::new(),
                derived: false,
            });
        }
        let key = project_key_of(issue_id)?;
        let project = self.with_cache(|cache| {
            cache
                .find_project(key)
                // Only a key match counts: an id here would be a coincidence.
                .filter(|p| tracker_core::unicode_eq_ignore_case(&p.short_name, key))
        })?;
        Some(IssueProject {
            id: project.id,
            derived: true,
        })
    }

    /// Ask the backend which project `issue_id` is in.
    pub fn issue_project_live(&self, issue_id: &str) -> Result<IssueProject> {
        if let Some(id) = self.issue_projects.borrow().get(issue_id) {
            return Ok(IssueProject {
                id: id.clone(),
                derived: false,
            });
        }
        let issue = self
            .client
            .get_issue(issue_id)
            .with_context(|| format!("Failed to fetch issue '{}'", issue_id))?;
        self.issue_projects
            .borrow_mut()
            .insert(issue_id.to_string(), issue.project.id.clone());
        Ok(IssueProject {
            id: issue.project.id,
            derived: false,
        })
    }
}

/// The project key in a readable issue id such as `PROJ-123`: everything
/// before a final `-<digits>`.
fn project_key_of(issue_id: &str) -> Option<&str> {
    let (key, number) = issue_id.rsplit_once('-')?;
    let is_number = !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
    (is_number && !key.is_empty()).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_keys_come_from_readable_ids_only() {
        assert_eq!(project_key_of("PROJ-123"), Some("PROJ"));
        assert_eq!(project_key_of("my-proj-7"), Some("my-proj"));
        assert_eq!(project_key_of("42"), None);
        assert_eq!(project_key_of("#42"), None);
        assert_eq!(project_key_of("PROJ-"), None);
        assert_eq!(project_key_of("-12"), None);
        assert_eq!(project_key_of("owner/repo#42"), None);
        assert_eq!(project_key_of("PROJ-12a"), None);
    }
}
