use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use tracker_core::http::{self, read_json};
use tracker_core::pagination::{Page, fetch_pages_to_end};
use tracker_core::{AttachmentUploadFile, LinkTypeMap, Result, TrackerError};
use ureq::typestate::WithBody;
use ureq::unversioned::multipart::Form;
use ureq::{Agent, RequestBuilder};

use crate::convert::GitLabQueryParams;
use crate::models::*;

/// GitLab's maximum `per_page`; also the page size used when walking every page.
const MAX_PER_PAGE: usize = 100;

/// Built-in GitLab link types, by canonical name (see [`LinkTypeMap`]).
const LINK_TYPE_DEFAULTS: &[(&str, &str)] = &[
    ("relates", "relates_to"),
    ("depends", "blocks"),
    ("required", "is_blocked_by"),
    ("duplicates", "relates_to"),
    ("duplicated-by", "relates_to"),
];

/// GitLab REST API client
pub struct GitLabClient {
    agent: Agent,
    base_url: String,
    token: String,
    project_id: Option<String>,
    pub(crate) link_types: LinkTypeMap,
}

/// The first of `message`, `error` and `error_description` that is a string;
/// GitLab's validation errors (an object under `message`) fall back to the raw body.
fn error_message(json: &Value) -> Option<String> {
    ["message", "error", "error_description"]
        .into_iter()
        .find_map(|key| json.get(key)?.as_str().map(str::to_string))
}

impl GitLabClient {
    /// Create a new GitLab client.
    ///
    /// `base_url` should include the API version path, e.g. `https://gitlab.com/api/v4`.
    /// `project_id` can be a numeric ID or a plain path like `group/sub/project`;
    /// it is URL-encoded when request URLs are built, so do not pre-encode it.
    pub fn new(base_url: &str, token: &str, project_id: Option<&str>) -> Self {
        Self {
            agent: http::agent(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            project_id: project_id.map(|s| s.to_string()),
            link_types: LinkTypeMap::new(LINK_TYPE_DEFAULTS, HashMap::new()),
        }
    }

    /// Set custom link type mappings (canonical name -> GitLab link type string)
    pub fn with_link_mappings(mut self, mappings: HashMap<String, String>) -> Self {
        self.link_types = LinkTypeMap::new(LINK_TYPE_DEFAULTS, mappings);
        self
    }

    /// Get the project ID string (or "unknown" if not configured).
    /// Used by trait_impl for building core models.
    pub(crate) fn project_id_str(&self) -> String {
        self.project_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// The configured project ID or path; an error if none is configured.
    fn configured_project_id(&self) -> Result<&str> {
        self.project_id.as_deref().ok_or_else(|| TrackerError::Api {
            status: 0,
            message: "GitLab project_id is not configured. Set via 'track config set gitlab.project_id <ID>' or GITLAB_PROJECT_ID env var".to_string(),
        })
    }

    /// Build a project-scoped URL. Returns an error if project_id is not configured.
    fn project_url(&self, path: &str) -> Result<String> {
        Ok(format!(
            "{}/projects/{}{}",
            self.base_url,
            urlencoding::encode(self.configured_project_id()?),
            path
        ))
    }

    /// Browser URL of the GitLab instance: the API base without `/api/v4`.
    pub(crate) fn instance_web_url(&self) -> &str {
        self.base_url
            .strip_suffix("/api/v4")
            .unwrap_or(&self.base_url)
    }

    /// Browser URL of the configured project, e.g. `https://gitlab.com/group/project`.
    pub(crate) fn project_web_url(&self) -> Result<String> {
        let project_id = self.configured_project_id()?;
        // A path is already the URL's tail; a numeric ID needs the API to learn it.
        if !project_id.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(format!(
                "{}/{}",
                self.instance_web_url(),
                project_id.trim_matches('/')
            ));
        }
        self.get_project(project_id)?
            .web_url
            .ok_or_else(|| TrackerError::Api {
                status: 0,
                message: format!("GitLab returned no web_url for project {project_id}"),
            })
    }

    /// Check response status and return error if not successful
    fn check_response(&self, response: http::Response) -> Result<http::Response> {
        http::check_response(response, error_message)
    }

    // ==================== HTTP Helpers ====================

    /// Add the auth and `Accept` headers every GitLab API request carries.
    fn authorized<B>(&self, request: RequestBuilder<B>) -> RequestBuilder<B> {
        request
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
    }

    /// GET `url`, failing on a non-2xx status.
    fn get(&self, url: &str) -> Result<http::Response> {
        self.check_response(self.authorized(self.agent.get(url)).call()?)
    }

    /// GET `url` and deserialize the JSON response body.
    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        read_json(self.get(url)?)
    }

    /// GET every page of the project-scoped collection at `path` (which must
    /// not carry a query string) and concatenate them in API order.
    ///
    /// GitLab honors `per_page`, so a short page is a reliable end-of-data
    /// signal; [`fetch_pages_to_end`]'s backstop catches a server that ignores paging.
    fn get_all_pages<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>> {
        let mut page = 1;
        fetch_pages_to_end(&format!("'{path}'"), || {
            let url = self.project_url(&format!("{path}?per_page={MAX_PER_PAGE}&page={page}"))?;
            let batch: Vec<T> = self.get_json(&url)?;
            page += 1;
            let last = batch.len() < MAX_PER_PAGE;
            Ok(Page { items: batch, last })
        })
    }

    /// Send `body` as JSON on a POST or PUT `request`, failing on a non-2xx status.
    fn send_json(
        &self,
        request: RequestBuilder<WithBody>,
        body: &impl Serialize,
    ) -> Result<http::Response> {
        let response = self
            .authorized(request)
            .header("Content-Type", "application/json")
            .send_json(body)?;
        self.check_response(response)
    }

    /// POST `body` as JSON and deserialize the JSON response body.
    fn post_json<T: DeserializeOwned>(&self, url: &str, body: &impl Serialize) -> Result<T> {
        read_json(self.send_json(self.agent.post(url), body)?)
    }

    /// PUT `body` as JSON and deserialize the JSON response body.
    fn put_json<T: DeserializeOwned>(&self, url: &str, body: &impl Serialize) -> Result<T> {
        read_json(self.send_json(self.agent.put(url), body)?)
    }

    /// DELETE `url`, failing on a non-2xx status. Any response body is ignored.
    fn delete(&self, url: &str) -> Result<()> {
        self.check_response(self.authorized(self.agent.delete(url)).call()?)?;
        Ok(())
    }

    /// POST `file` as the multipart `file` field and deserialize the JSON response body.
    fn post_file<T: DeserializeOwned>(&self, url: &str, file: &AttachmentUploadFile) -> Result<T> {
        let form = Form::new().part("file", http::file_part(file)?);

        let response = self.authorized(self.agent.post(url)).send(form)?;
        read_json(self.check_response(response)?)
    }

    // ==================== Issue Operations ====================

    /// Get an issue by IID (project-scoped number)
    pub fn get_issue(&self, iid: u64) -> Result<GitLabIssue> {
        self.get_json(&self.project_url(&format!("/issues/{}", iid))?)
    }

    fn append_assignee_filter(url: &mut String, assignee_username: Option<&str>) {
        let Some(raw) = assignee_username.filter(|s| !s.is_empty()) else {
            return;
        };
        if raw == "@me" || raw.eq_ignore_ascii_case("me") {
            // Issues API: current user is scope=assigned_to_me, not assignee_id=me.
            url.push_str("&scope=assigned_to_me");
        } else {
            let name = raw.strip_prefix('@').unwrap_or(raw);
            // Documented type is string array (`assignee_username[]`); CE allows one value.
            url.push_str(&format!(
                "&assignee_username[]={}",
                urlencoding::encode(name)
            ));
        }
    }

    /// Fetch one page of project issues matching `query`, plus the `X-Total`
    /// match count when GitLab reports it (it omits it above 10,000 matches).
    pub(crate) fn fetch_project_issues(
        &self,
        query: &GitLabQueryParams,
        per_page: usize,
        page: usize,
    ) -> Result<(Vec<GitLabIssue>, Option<u64>)> {
        let mut url = self.project_url(&format!("/issues?per_page={}&page={}", per_page, page))?;
        let filters = [
            (
                "search",
                Some(query.search.as_str()).filter(|s| !s.is_empty()),
            ),
            ("state", query.state.as_deref()),
            ("labels", query.labels.as_deref()),
            ("order_by", query.order_by.as_deref()),
            ("sort", query.sort.as_deref()),
        ];
        for (key, value) in filters {
            if let Some(value) = value {
                url.push_str(&format!("&{}={}", key, urlencoding::encode(value)));
            }
        }
        Self::append_assignee_filter(&mut url, query.assignee_username.as_deref());

        let response = self.get(&url)?;
        let total = response
            .headers()
            .get("x-total")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let issues: Vec<GitLabIssue> = read_json(response)?;
        Ok((issues, total))
    }

    /// Create a new issue
    pub fn create_issue(&self, issue: &CreateGitLabIssue) -> Result<GitLabIssue> {
        self.post_json(&self.project_url("/issues")?, issue)
    }

    /// Update an existing issue (GitLab uses PUT, not PATCH)
    pub fn update_issue(&self, iid: u64, update: &UpdateGitLabIssue) -> Result<GitLabIssue> {
        self.put_json(&self.project_url(&format!("/issues/{}", iid))?, update)
    }

    /// Delete an issue
    pub fn delete_issue(&self, iid: u64) -> Result<()> {
        self.delete(&self.project_url(&format!("/issues/{}", iid))?)
    }

    /// Upload a file to the project markdown uploads endpoint.
    pub fn upload_project_file(&self, file: &AttachmentUploadFile) -> Result<GitLabUpload> {
        self.post_file(&self.project_url("/uploads")?, file)
    }

    // ==================== Project Operations ====================

    /// List projects the authenticated user is a member of
    pub fn list_projects(&self) -> Result<Vec<GitLabProject>> {
        self.get_json(&format!(
            "{}/projects?membership=true&per_page=100&order_by=updated_at",
            self.base_url
        ))
    }

    /// Get a project by numeric ID or path (`group/project`)
    pub fn get_project(&self, id: &str) -> Result<GitLabProject> {
        self.get_json(&format!(
            "{}/projects/{}",
            self.base_url,
            urlencoding::encode(id)
        ))
    }

    // ==================== Label Operations ====================

    /// List every label of the project (all pages)
    pub fn list_labels(&self) -> Result<Vec<GitLabLabel>> {
        self.get_all_pages("/labels")
    }

    /// Create a new label
    pub fn create_label(&self, label: &CreateGitLabLabel) -> Result<GitLabLabel> {
        self.post_json(&self.project_url("/labels")?, label)
    }

    /// Delete a label by ID
    pub fn delete_label(&self, label_id: u64) -> Result<()> {
        self.delete(&self.project_url(&format!("/labels/{}", label_id))?)
    }

    /// Update a label by ID
    pub fn update_label(&self, label_id: u64, update: &UpdateGitLabLabel) -> Result<GitLabLabel> {
        self.put_json(&self.project_url(&format!("/labels/{}", label_id))?, update)
    }

    // ==================== Note (Comment) Operations ====================

    /// Add a note to an issue
    pub fn add_note(&self, iid: u64, body: &str) -> Result<GitLabNote> {
        let note = CreateGitLabNote {
            body: body.to_string(),
        };
        self.post_json(&self.project_url(&format!("/issues/{}/notes", iid))?, &note)
    }

    /// Get a native GitLab notes page on an issue.
    pub(crate) fn get_notes_page_raw(
        &self,
        iid: u64,
        per_page: usize,
        page: usize,
    ) -> Result<Vec<GitLabNote>> {
        self.get_json(&self.project_url(&format!(
            "/issues/{}/notes?per_page={}&page={}",
            iid,
            per_page.min(MAX_PER_PAGE),
            page.max(1)
        ))?)
    }

    // ==================== Resource Event (History) Operations ====================

    /// Fetch all of an issue's resource state events (oldest-first).
    pub fn get_state_events(&self, iid: u64) -> Result<Vec<GitLabStateEvent>> {
        self.get_all_pages(&format!("/issues/{}/resource_state_events", iid))
    }

    /// Fetch all of an issue's resource label events (oldest-first).
    pub fn get_label_events(&self, iid: u64) -> Result<Vec<GitLabLabelEvent>> {
        self.get_all_pages(&format!("/issues/{}/resource_label_events", iid))
    }

    /// Fetch all of an issue's resource milestone events (oldest-first).
    pub fn get_milestone_events(&self, iid: u64) -> Result<Vec<GitLabMilestoneEvent>> {
        self.get_all_pages(&format!("/issues/{}/resource_milestone_events", iid))
    }

    // ==================== Link Operations ====================

    /// Get issue links for an issue
    pub fn get_issue_links(&self, iid: u64) -> Result<Vec<GitLabIssueLink>> {
        self.get_json(&self.project_url(&format!("/issues/{}/links", iid))?)
    }

    /// Create an issue link
    pub fn create_issue_link(&self, iid: u64, link: &CreateGitLabIssueLink) -> Result<()> {
        let url = self.project_url(&format!("/issues/{}/links", iid))?;
        self.send_json(self.agent.post(&url), link)?;
        Ok(())
    }

    /// Delete an issue link by issue IID and link ID
    pub fn delete_issue_link(&self, iid: u64, issue_link_id: u64) -> Result<()> {
        self.delete(&self.project_url(&format!("/issues/{}/links/{}", iid, issue_link_id))?)
    }

    // ==================== Work Item (GraphQL) Operations ====================

    /// Derive the GraphQL endpoint URL from the REST API base URL.
    ///
    /// The base_url is expected to end with `/api/v4` (e.g. `https://gitlab.com/api/v4`).
    /// The GraphQL endpoint lives at `/api/graphql`.
    fn graphql_url(&self) -> String {
        if let Some(prefix) = self.base_url.strip_suffix("/api/v4") {
            format!("{}/api/graphql", prefix)
        } else {
            // Fallback: append /graphql to whatever base we have
            format!("{}/graphql", self.base_url)
        }
    }

    /// Set the parent of a work item (issue) using the GitLab GraphQL API.
    ///
    /// Both IDs must be **global** numeric IDs (not project-scoped IIDs).
    /// The mutation uses the `hierarchyWidget` on `workItemUpdate`.
    pub fn set_work_item_parent(&self, child_global_id: u64, parent_global_id: u64) -> Result<()> {
        let query = format!(
            r#"mutation {{ workItemUpdate(input: {{ id: "gid://gitlab/Issue/{}", hierarchyWidget: {{ parentId: "gid://gitlab/Issue/{}" }} }}) {{ workItem {{ id }} errors }} }}"#,
            child_global_id, parent_global_id
        );
        let body = serde_json::json!({ "query": query });
        let result: serde_json::Value = self.post_json(&self.graphql_url(), &body)?;

        // Check for top-level GraphQL errors
        if let Some(errors) = result.get("errors")
            && let Some(arr) = errors.as_array()
            && !arr.is_empty()
        {
            let msg = arr
                .iter()
                .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(TrackerError::Api {
                status: 0,
                message: format!("GraphQL error: {}", msg),
            });
        }

        // Check for mutation-level errors (workItemUpdate.errors)
        if let Some(data) = result.get("data")
            && let Some(update) = data.get("workItemUpdate")
            && let Some(errors) = update.get("errors")
            && let Some(arr) = errors.as_array()
            && !arr.is_empty()
        {
            let msg = arr
                .iter()
                .filter_map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(TrackerError::Api {
                status: 0,
                message: format!("Failed to set parent: {}", msg),
            });
        }

        Ok(())
    }

    // ==================== Member Operations ====================

    /// List every project member, including inherited members (all pages)
    pub fn list_project_members(&self) -> Result<Vec<GitLabUser>> {
        self.get_all_pages("/members/all")
    }

    /// Resolve a login or numeric id to a verified GitLab user id.
    ///
    /// Accepted forms:
    /// - `@username` or `username` (case-insensitive match): prefer project
    ///   members (including inherited), then `GET /users?username=`.
    /// - numeric user id: verified against `GET /users/:id` before use, so a
    ///   typo cannot be silently dropped by GitLab's create/update endpoints
    ///   (they ignore unknown `assignee_ids`).
    ///
    /// Empty or `"0"` is not a lookup: callers treat those as a request to
    /// clear assignees.
    pub fn find_user_id(&self, username: &str) -> Result<u64> {
        let username = username.strip_prefix('@').unwrap_or(username);
        if let Ok(id) = username.parse::<u64>() {
            return self.verify_user_id(id);
        }

        // Only the first page of members is checked: it is a fast path, and
        // walking every page of a large group's inherited members would cost
        // far more requests than the authoritative `/users` lookup below.
        let members: Vec<GitLabUser> =
            self.get_json(&self.project_url(&format!("/members/all?per_page={MAX_PER_PAGE}"))?)?;
        if let Some(member) = members
            .into_iter()
            .find(|user| tracker_core::unicode_eq_ignore_case(&user.username, username))
        {
            return Ok(member.id);
        }

        let users: Vec<GitLabUser> = self.get_json(&format!(
            "{}/users?username={}",
            self.base_url,
            urlencoding::encode(username)
        ))?;
        users
            .into_iter()
            .find(|user| tracker_core::unicode_eq_ignore_case(&user.username, username))
            .map(|user| user.id)
            .ok_or_else(|| TrackerError::Api {
                status: 404,
                message: format!("GitLab user '{username}' not found"),
            })
    }

    /// Verify that a numeric user id exists via `GET /users/:id` and that the
    /// returned identity matches the requested id.
    fn verify_user_id(&self, id: u64) -> Result<u64> {
        let user: GitLabUser = self.get_json(&format!("{}/users/{}", self.base_url, id))?;
        if user.id != id {
            return Err(TrackerError::Api {
                status: 404,
                message: format!(
                    "GitLab user id '{id}' resolved to a different user (id {})",
                    user.id
                ),
            });
        }
        Ok(user.id)
    }

    // ==================== Wiki Operations ====================

    /// List project wiki pages.
    ///
    /// GitLab's `GET /projects/:id/wikis` endpoint does not support
    /// pagination (`per_page`/`page` are not documented parameters and are
    /// silently ignored -- confirmed against the live API, which returns
    /// the full collection in a single response regardless). Callers that
    /// need a subset should slice the returned `Vec` themselves.
    pub fn list_wiki_pages(&self, with_content: bool) -> Result<Vec<GitLabWikiPage>> {
        self.get_json(&self.project_url(&format!(
            "/wikis?with_content={}",
            if with_content { 1 } else { 0 }
        ))?)
    }

    /// Get a project wiki page by slug.
    pub fn get_wiki_page(&self, slug: &str) -> Result<GitLabWikiPage> {
        self.get_json(&self.project_url(&format!("/wikis/{}", urlencoding::encode(slug)))?)
    }

    /// Create a project wiki page.
    pub fn create_wiki_page(&self, page: &CreateGitLabWikiPage) -> Result<GitLabWikiPage> {
        self.post_json(&self.project_url("/wikis")?, page)
    }

    /// Update a project wiki page.
    pub fn update_wiki_page(
        &self,
        slug: &str,
        update: &UpdateGitLabWikiPage,
    ) -> Result<GitLabWikiPage> {
        self.put_json(
            &self.project_url(&format!("/wikis/{}", urlencoding::encode(slug)))?,
            update,
        )
    }

    /// Delete a project wiki page.
    pub fn delete_wiki_page(&self, slug: &str) -> Result<()> {
        self.delete(&self.project_url(&format!("/wikis/{}", urlencoding::encode(slug)))?)
    }

    /// Upload one file to project wiki attachments.
    pub fn upload_wiki_attachment(
        &self,
        file: &AttachmentUploadFile,
    ) -> Result<GitLabWikiAttachment> {
        self.post_file(&self.project_url("/wikis/attachments")?, file)
    }
}
