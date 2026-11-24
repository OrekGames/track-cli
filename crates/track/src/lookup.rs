//! Project lookups that read through the local cache.
//!
//! Commands need a project's id and schema to build and check a write. Both
//! change rarely, so [`Lookup`] answers from the cache when it can and
//! writes back what it had to fetch. It is also how commands keep the cache
//! in step with the changes they make.
//!
//! The cache only ever shortens the happy path. A caller that gets a "no"
//! out of cached data (unknown field, invalid value) asks again with the
//! `*_live` methods before it reports an error, so stale data cannot reject
//! something the tracker would accept. A cached project id gets the same
//! second chance from [`Lookup::with_project_id`].

use crate::cache::TrackerCache;
use anyhow::{Context, Result};
use chrono::Duration;
use std::borrow::Cow;
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use tracker_core::{
    CreateIssue, Issue, IssueTag, IssueTracker, Project, ProjectCustomField, TrackerError, case_key,
};

/// How long a cached schema or project id answers lookups.
const DEFAULT_TTL_HOURS: i64 = 24;
/// Overrides the lifetime above, in `cache refresh --if-stale` syntax (`30m`, `7d`).
const TTL_ENV: &str = "TRACK_SCHEMA_TTL";

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

/// A project id and where it came from.
struct KnownProjectId {
    /// The key, name or id the command gave, to ask the backend again with.
    identifier: String,
    id: String,
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
    /// The cache for this backend and instance, opened on first use. `None`
    /// inside when there is none.
    cache: OnceCell<Option<RefCell<TrackerCache>>>,
    /// Whether lookups may read the cache and write back what they fetch.
    /// Off with `--no-cache`. The changes a command makes reach the cache
    /// either way, so that it never keeps what the tracker no longer has.
    use_cache: bool,
    ttl: Duration,
    /// Project ids by identifier, case-folded as the cache keys them.
    project_ids: RefCell<HashMap<String, KnownProjectId>>,
    schemas: RefCell<HashMap<String, KnownSchema>>,
    issue_projects: RefCell<HashMap<String, String>>,
}

impl<'a> Lookup<'a> {
    /// Lookups against `client`; `use_cache` is false for `--no-cache`.
    pub fn new(client: &'a dyn IssueTracker, use_cache: bool) -> Self {
        Self {
            client,
            cache: OnceCell::new(),
            use_cache,
            ttl: ttl(),
            project_ids: RefCell::new(HashMap::new()),
            schemas: RefCell::new(HashMap::new()),
            issue_projects: RefCell::new(HashMap::new()),
        }
    }

    pub fn client(&self) -> &'a dyn IssueTracker {
        self.client
    }

    /// The cache for this backend and instance, if there is one.
    fn cache(&self) -> Option<&RefCell<TrackerCache>> {
        self.cache
            .get_or_init(|| TrackerCache::open_active().map(RefCell::new))
            .as_ref()
    }

    /// Run `f` on the cache for a lookup, unless lookups skip the cache.
    fn with_cache<T>(&self, f: impl FnOnce(&mut TrackerCache) -> Option<T>) -> Option<T> {
        if !self.use_cache {
            return None;
        }
        self.cache().and_then(|cache| f(&mut cache.borrow_mut()))
    }

    /// Record in the cache something this command changed. Failures are
    /// ignored: the cache is a convenience and must not fail the command
    /// that feeds it.
    fn record<T>(&self, change: impl FnOnce(&mut TrackerCache) -> T) {
        if let Some(cache) = self.cache() {
            let _ = change(&mut cache.borrow_mut());
        }
    }

    /// Resolve a project key, name or id to the backend's project id: from
    /// this run, then the cache, then the backend.
    pub fn project_id(&self, identifier: &str) -> Result<String> {
        if let Some(known) = self.project_ids.borrow().get(&case_key(identifier)) {
            return Ok(known.id.clone());
        }
        if let Some(id) = self.with_cache(|cache| cache.resolved_project_id(identifier, self.ttl)) {
            self.remember_project_id(identifier, &id, false);
            return Ok(id);
        }
        self.project_id_live(identifier)
    }

    /// Ask the backend for a project's id, and remember the answer.
    fn project_id_live(&self, identifier: &str) -> Result<String> {
        let id = self
            .client
            .resolve_project_id(identifier)
            .with_context(|| format!("Failed to resolve project '{}'", identifier))?;
        self.with_cache(|cache| cache.store_resolved_project_id(identifier, &id).ok());
        self.remember_project_id(identifier, &id, true);
        Ok(id)
    }

    fn remember_project_id(&self, identifier: &str, id: &str, live: bool) {
        self.project_ids.borrow_mut().insert(
            case_key(identifier),
            KnownProjectId {
                identifier: identifier.to_string(),
                id: id.to_string(),
                live,
            },
        );
    }

    /// Run `request` with `project_id`, an id [`Self::project_id`] gave.
    ///
    /// An id from the cache can be out of date: a project deleted and
    /// recreated has a new one. So when the tracker turns down a request
    /// made with a cached id, the backend is asked for the id again. If it
    /// changed, the cache learns the new one and `request` runs once more
    /// with it. If the backend no longer knows the project, the cached id
    /// is forgotten and that is the error.
    pub fn with_project_id<T>(
        &self,
        project_id: &str,
        request: impl Fn(&str) -> Result<T>,
    ) -> Result<T> {
        let err = match request(project_id) {
            Ok(value) => return Ok(value),
            Err(err) => err,
        };
        let Some(identifier) = self
            .cached_identifier_of(project_id)
            .filter(|_| turned_down(&err))
        else {
            return Err(err);
        };
        // Forgotten first, so that if asking fails the next run asks too.
        self.with_cache(|cache| cache.forget_resolved_project_id(&identifier).ok());
        let renewed = self.project_id_live(&identifier)?;
        if renewed == project_id {
            // The id was right; the request failed for another reason.
            return Err(err);
        }
        request(&renewed)
    }

    /// How to name `project_id` to the user: the key, name or id the
    /// command gave for it, else the id itself. `None` for the empty id of a
    /// backend whose schema belongs to no project.
    fn named_as(&self, project_id: &str) -> Option<String> {
        let given = self
            .project_ids
            .borrow()
            .values()
            .filter(|known| known.id == project_id)
            .map(|known| known.identifier.clone())
            .min();
        given.or_else(|| (!project_id.is_empty()).then(|| project_id.to_string()))
    }

    /// The identifier a cached `project_id` was looked up by in this run.
    fn cached_identifier_of(&self, project_id: &str) -> Option<String> {
        self.project_ids
            .borrow()
            .values()
            .find(|known| !known.live && known.id == project_id)
            .map(|known| known.identifier.clone())
    }

    /// Create an issue, giving its project id the second chance
    /// [`Self::with_project_id`] describes.
    pub fn create_issue(&self, create: &CreateIssue) -> Result<Issue> {
        self.with_project_id(&create.project_id, |project_id| {
            let create = if project_id == create.project_id {
                Cow::Borrowed(create)
            } else {
                Cow::Owned(CreateIssue {
                    project_id: project_id.to_string(),
                    ..create.clone()
                })
            };
            self.client
                .create_issue(&create)
                .context("Failed to create issue")
        })
    }

    /// Where this run keeps a project's schema. Backends whose schema is
    /// the same for every project keep one, whatever id names it.
    fn schema_key<'k>(&self, project_id: &'k str) -> &'k str {
        if self.client.schema_is_project_independent() {
            ""
        } else {
            project_id
        }
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
        if let Some(known) = self.schemas.borrow().get(self.schema_key(project_id)) {
            return Some(known.fields.clone());
        }
        if self.client.schema_is_project_independent() {
            // Built in, not fetched: nothing worth writing to the cache.
            return self.fetch_schema(project_id).ok();
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

    /// Fetch a project's schema from the backend, remember it, and write it
    /// back to the cache.
    pub fn schema_live(&self, project_id: &str) -> Result<Schema> {
        let fields = self.fetch_schema(project_id)?;
        if !self.client.schema_is_project_independent() {
            self.with_cache(|cache| cache.store_project_schema(project_id, &fields).ok());
        }
        Ok(fields)
    }

    /// Fetch a project's schema from the backend and remember it for this run.
    fn fetch_schema(&self, project_id: &str) -> Result<Schema> {
        let fields: Schema = self
            .client
            .get_project_custom_fields(project_id)
            .with_context(|| match self.named_as(project_id) {
                Some(project) => format!("Failed to fetch custom fields for project '{project}'"),
                None => "Failed to fetch custom fields".to_string(),
            })?
            .into();
        self.schemas.borrow_mut().insert(
            self.schema_key(project_id).to_string(),
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
            .get(self.schema_key(project_id))
            .is_some_and(|known| !known.live)
    }

    /// Forget a project's schema after a change to it, here and in the cache.
    pub fn forget_schema(&self, project_id: &str) {
        self.schemas
            .borrow_mut()
            .remove(self.schema_key(project_id));
        self.record(|cache| cache.invalidate_project_schema(project_id));
    }

    /// Forget every project's schema, after a change that may touch any.
    pub fn forget_all_schemas(&self) {
        self.schemas.borrow_mut().clear();
        self.record(TrackerCache::invalidate_all_schemas);
    }

    /// Write back a tag list just read from the backend.
    pub fn store_tags(&self, tags: &[IssueTag]) {
        self.with_cache(|cache| cache.store_tags(tags).ok());
    }

    /// Record a tag just created or changed; `replacing` is its name before
    /// a rename.
    pub fn record_tag(&self, tag: &IssueTag, replacing: Option<&str>) {
        self.record(|cache| cache.upsert_tag(tag, replacing));
    }

    /// Record that a tag was deleted.
    pub fn record_tag_deleted(&self, name: &str) {
        self.record(|cache| cache.remove_tag(name));
    }

    /// Record a project just created.
    pub fn record_project(&self, project: &Project) {
        self.record(|cache| cache.upsert_project_meta(project));
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

/// Whether the tracker refused a request as it was put, the way it refuses
/// one naming a project id that no longer exists. Not being able to reach
/// the tracker, or to get in, does not count.
fn turned_down(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<TrackerError>())
        .any(|err| {
            matches!(
                err.code(),
                "not_found" | "project_not_found" | "invalid_input" | "api"
            )
        })
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
    use std::sync::Mutex;
    use tracker_core::{Comment, IssueLink, SearchResult, UpdateIssue};

    /// A tracker that answers project lookups and records what it was asked.
    #[derive(Default)]
    struct Tracker {
        /// What `resolve_project_id` answers.
        project_id: String,
        independent: bool,
        /// Whether `get_project_custom_fields` fails.
        schema_fails: bool,
        calls: Mutex<Vec<String>>,
    }

    impl Tracker {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn log(&self, call: String) {
            self.calls.lock().unwrap().push(call);
        }
    }

    impl IssueTracker for Tracker {
        fn resolve_project_id(&self, identifier: &str) -> tracker_core::Result<String> {
            self.log(format!("resolve {identifier}"));
            Ok(self.project_id.clone())
        }
        fn get_project_custom_fields(
            &self,
            project_id: &str,
        ) -> tracker_core::Result<Vec<ProjectCustomField>> {
            self.log(format!("schema {project_id}"));
            if self.schema_fails {
                return Err(TrackerError::Unauthorized);
            }
            Ok(Vec::new())
        }
        fn schema_is_project_independent(&self) -> bool {
            self.independent
        }
        fn get_issue(&self, _: &str) -> tracker_core::Result<Issue> {
            unimplemented!()
        }
        fn search_issues(
            &self,
            _: &str,
            _: usize,
            _: usize,
        ) -> tracker_core::Result<SearchResult<Issue>> {
            unimplemented!()
        }
        fn create_issue(&self, _: &CreateIssue) -> tracker_core::Result<Issue> {
            unimplemented!()
        }
        fn update_issue(&self, _: &str, _: &UpdateIssue) -> tracker_core::Result<Issue> {
            unimplemented!()
        }
        fn delete_issue(&self, _: &str) -> tracker_core::Result<()> {
            unimplemented!()
        }
        fn list_projects(&self) -> tracker_core::Result<Vec<Project>> {
            unimplemented!()
        }
        fn get_project(&self, _: &str) -> tracker_core::Result<Project> {
            unimplemented!()
        }
        fn create_project(&self, _: &tracker_core::CreateProject) -> tracker_core::Result<Project> {
            unimplemented!()
        }
        fn list_tags(&self) -> tracker_core::Result<Vec<IssueTag>> {
            unimplemented!()
        }
        fn get_issue_links(&self, _: &str) -> tracker_core::Result<Vec<IssueLink>> {
            unimplemented!()
        }
        fn link_issues(&self, _: &str, _: &str, _: &str, _: &str) -> tracker_core::Result<()> {
            unimplemented!()
        }
        fn link_subtask(&self, _: &str, _: &str) -> tracker_core::Result<()> {
            unimplemented!()
        }
        fn add_comment(&self, _: &str, _: &str) -> tracker_core::Result<Comment> {
            unimplemented!()
        }
        fn get_comments(&self, _: &str) -> tracker_core::Result<Vec<Comment>> {
            unimplemented!()
        }
    }

    /// A request that the tracker answers with `outcome` for project `0-1`
    /// and accepts for any other id.
    fn request_failing_with<'t>(
        tracker: &'t Tracker,
        outcome: fn() -> TrackerError,
    ) -> impl Fn(&str) -> Result<String> + 't {
        move |project_id| {
            tracker.log(format!("request {project_id}"));
            if project_id == "0-1" {
                Err(outcome()).context("Failed to create issue")
            } else {
                Ok(project_id.to_string())
            }
        }
    }

    #[test]
    fn project_ids_are_resolved_once_per_run() {
        let tracker = Tracker {
            project_id: "0-1".to_string(),
            ..Default::default()
        };
        let lookup = Lookup::new(&tracker, false);

        assert_eq!(lookup.project_id("DEMO").unwrap(), "0-1");
        assert_eq!(lookup.project_id("demo").unwrap(), "0-1");
        assert_eq!(tracker.calls(), ["resolve DEMO"]);
    }

    #[test]
    fn a_cached_id_the_tracker_turns_down_is_asked_for_again() {
        let tracker = Tracker {
            project_id: "0-5".to_string(),
            ..Default::default()
        };
        let lookup = Lookup::new(&tracker, false);
        // As if the cache had answered.
        lookup.remember_project_id("DEMO", "0-1", false);

        let not_found = || TrackerError::Api {
            status: 404,
            message: "Entity with id 0-1 not found".to_string(),
        };
        let done = lookup
            .with_project_id("0-1", request_failing_with(&tracker, not_found))
            .unwrap();

        assert_eq!(done, "0-5");
        assert_eq!(
            tracker.calls(),
            ["request 0-1", "resolve DEMO", "request 0-5"]
        );
        // The new id is what the rest of the run uses.
        assert_eq!(lookup.project_id("DEMO").unwrap(), "0-5");
    }

    #[test]
    fn a_tracker_that_cannot_be_reached_is_not_asked_again() {
        let tracker = Tracker {
            project_id: "0-5".to_string(),
            ..Default::default()
        };
        let lookup = Lookup::new(&tracker, false);
        lookup.remember_project_id("DEMO", "0-1", false);

        let offline = || TrackerError::Http("connection reset".to_string());
        assert!(
            lookup
                .with_project_id("0-1", request_failing_with(&tracker, offline))
                .is_err()
        );
        let refused = || TrackerError::Unauthorized;
        assert!(
            lookup
                .with_project_id("0-1", request_failing_with(&tracker, refused))
                .is_err()
        );
        assert_eq!(tracker.calls(), ["request 0-1", "request 0-1"]);
    }

    #[test]
    fn an_id_from_the_backend_gets_no_second_chance() {
        let tracker = Tracker {
            project_id: "0-1".to_string(),
            ..Default::default()
        };
        let lookup = Lookup::new(&tracker, false);
        let project_id = lookup.project_id("DEMO").unwrap();

        let not_found = || TrackerError::ProjectNotFound("0-1".to_string());
        assert!(
            lookup
                .with_project_id(&project_id, request_failing_with(&tracker, not_found))
                .is_err()
        );
        assert_eq!(tracker.calls(), ["resolve DEMO", "request 0-1"]);
    }

    #[test]
    fn a_schema_shared_by_every_project_is_fetched_once() {
        let tracker = Tracker {
            independent: true,
            ..Default::default()
        };
        let lookup = Lookup::new(&tracker, false);

        // An update knows no project; a create names one.
        assert!(lookup.schema_if_free("").is_some());
        assert!(lookup.schema("owner/repo").is_ok());
        assert!(!lookup.schema_may_be_stale("owner/repo"));
        assert_eq!(tracker.calls(), ["schema "]);
    }

    #[test]
    fn a_failed_schema_fetch_names_the_project_as_it_was_given() {
        let tracker = Tracker {
            project_id: "0-1".to_string(),
            schema_fails: true,
            ..Default::default()
        };
        let lookup = Lookup::new(&tracker, false);
        let project_id = lookup.project_id("DEMO").unwrap();

        let err = lookup.schema_live(&project_id).unwrap_err();
        let message = format!("{err:#}");
        assert!(
            message.starts_with("Failed to fetch custom fields for project 'DEMO': "),
            "{message}"
        );
        assert!(!message.contains("0-1"), "{message}");

        // An id no identifier of this run maps to is named as it is.
        let err = lookup.schema_live("0-9").unwrap_err();
        assert!(
            format!("{err:#}").starts_with("Failed to fetch custom fields for project '0-9': "),
            "{err:#}"
        );
    }

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
