use crate::error::{Result, YouTrackError};
use crate::models::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::time::Duration;
use tracker_core::{AttachmentUpload, unicode_eq_ignore_case};
use ureq::Agent;
use ureq::http::Response;
use ureq::unversioned::multipart::{Form, Part};

const DEFAULT_ISSUE_FIELDS: &str = "id,idReadable,summary,description,project(id,name,shortName),customFields(name,$type,value(name,login,isResolved,text,minutes,presentation,color(id))),tags(id,name),created,updated,resolved";
const DEFAULT_PROJECT_FIELDS: &str = "id,name,shortName,description";
const DEFAULT_ARTICLE_FIELDS: &str = "id,idReadable,summary,content,project(id,name,shortName),parentArticle(id,idReadable,summary),hasChildren,tags(id,name),created,updated,reporter(login,name)";
const USER_FIELDS: &str = "id,login,fullName";
const TAG_FIELDS: &str = "id,name,color(id,background,foreground)";
const COMMENT_FIELDS: &str = "id,text,author(login,name),created";
const ISSUE_ATTACHMENT_FIELDS: &str =
    "id,name,size,mimeType,url,created,author(login,name),comment(id)";
const ARTICLE_ATTACHMENT_FIELDS: &str = "id,name,size,mimeType,url,created,author(login,name)";

pub struct YouTrackClient {
    agent: Agent,
    base_url: String,
    token: String,
    link_mappings: HashMap<String, String>,
}

/// Decode a JSON response body.
fn read_json<T: DeserializeOwned>(mut response: Response<ureq::Body>) -> Result<T> {
    Ok(response.body_mut().read_json()?)
}

impl YouTrackClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();

        Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            link_mappings: HashMap::new(),
        }
    }

    /// Set custom link type mappings (canonical name -> YouTrack link type name)
    pub fn with_link_mappings(mut self, mappings: HashMap<String, String>) -> Self {
        self.link_mappings = mappings;
        self
    }

    /// Resolve a canonical link type name to the YouTrack-native link type name.
    /// User overrides take precedence, then built-in defaults, then pass-through.
    pub(crate) fn resolve_link_type(&self, canonical: &str) -> String {
        if let Some(name) = self.link_mappings.get(canonical) {
            return name.clone();
        }
        match canonical {
            "relates" => "Relates",
            "depends" => "Depend",
            "required" => "Depend",
            "duplicates" => "Duplicate",
            "duplicated-by" => "Duplicate",
            _ => canonical,
        }
        .to_string()
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// Check response status and return error with body details if not successful
    fn check_response(&self, mut response: Response<ureq::Body>) -> Result<Response<ureq::Body>> {
        let status = response.status().as_u16();

        if (200..300).contains(&status) {
            return Ok(response);
        }

        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| String::new());

        let message = if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&body) {
            let mut parts = Vec::new();

            if let Some(error) = error_json.get("error").and_then(|v| v.as_str()) {
                parts.push(error.to_string());
            }

            if let Some(desc) = error_json.get("error_description").and_then(|v| v.as_str()) {
                parts.push(desc.to_string());
            }

            if let Some(msg) = error_json.get("error_message").and_then(|v| v.as_str()) {
                parts.push(msg.to_string());
            }

            if parts.is_empty() {
                body
            } else {
                parts.join(": ")
            }
        } else if body.is_empty() {
            format!("HTTP {}", status)
        } else {
            body
        };

        match status {
            401 => Err(YouTrackError::Unauthorized),
            _ => Err(YouTrackError::Api { status, message }),
        }
    }

    /// GET a JSON endpoint and decode the response.
    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self
            .agent
            .get(url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()?;

        read_json(self.check_response(response)?)
    }

    /// GET a JSON endpoint, returning None on 404 and propagating other errors.
    fn get_json_opt<T: DeserializeOwned>(&self, url: &str) -> Result<Option<T>> {
        match self.get_json(url) {
            Ok(value) => Ok(Some(value)),
            Err(YouTrackError::Api { status: 404, .. }) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// POST a JSON body, returning the response once its status is checked.
    fn post(&self, url: &str, body: &impl Serialize) -> Result<Response<ureq::Body>> {
        let response = self
            .agent
            .post(url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(body)?;

        self.check_response(response)
    }

    /// POST a JSON body and decode the JSON response.
    fn post_json<T: DeserializeOwned>(&self, url: &str, body: &impl Serialize) -> Result<T> {
        read_json(self.post(url, body)?)
    }

    /// DELETE a resource, discarding the response body.
    fn delete(&self, url: &str) -> Result<()> {
        let response = self
            .agent
            .delete(url)
            .header("Authorization", &self.auth_header())
            .call()?;

        self.check_response(response)?;
        Ok(())
    }

    /// POST `upload.files` as a multipart form under YouTrack's `upload` field,
    /// muting update notifications when `upload.silent` is set.
    fn post_attachment_form<T: DeserializeOwned>(
        &self,
        url: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<T>> {
        let mut url = url.to_string();
        if upload.silent {
            url.push_str("&muteUpdateNotifications=true");
        }

        let mut form = Form::new();
        for file in &upload.files {
            let mut part = Part::file(&file.path)?;
            if let Some(name) = &file.name {
                part = part.file_name(name);
            }
            if let Some(mime_type) = &file.mime_type {
                part = part.mime_str(mime_type)?;
            }
            form = form.part("upload", part);
        }

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .send(form)?;

        read_json(self.check_response(response)?)
    }

    pub fn get_issue(&self, id: &str) -> Result<Issue> {
        self.get_json(&format!(
            "{}/api/issues/{}?fields={}",
            self.base_url, id, DEFAULT_ISSUE_FIELDS
        ))
    }

    pub fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Issue>> {
        self.get_json(&format!(
            "{}/api/issues?query={}&fields={}&$top={}&$skip={}",
            self.base_url,
            urlencoding::encode(query),
            DEFAULT_ISSUE_FIELDS,
            limit,
            skip
        ))
    }

    /// Count issues matching a query via POST /api/issuesGetter/count.
    ///
    /// YouTrack counts asynchronously -- if count == -1 the server is still
    /// processing. Retries up to MAX_RETRIES times with a short sleep between
    /// attempts. Returns None if counting never completed.
    pub fn count_issues(&self, query: &str) -> Result<Option<u64>> {
        const MAX_RETRIES: u32 = 4;
        const RETRY_DELAY_MS: u64 = 400;

        self.count_issues_with_retry(query, MAX_RETRIES, Duration::from_millis(RETRY_DELAY_MS))
    }

    pub(crate) fn count_issues_with_retry(
        &self,
        query: &str,
        max_attempts: u32,
        retry_delay: Duration,
    ) -> Result<Option<u64>> {
        let url = format!("{}/api/issuesGetter/count?fields=count", self.base_url);
        let body = serde_json::json!({ "query": query });

        for attempt in 0..max_attempts {
            let count_resp: IssueCountResponse = self.post_json(&url, &body)?;

            if count_resp.count >= 0 {
                return Ok(Some(count_resp.count as u64));
            }

            // -1 means still counting; wait and retry (except on last attempt)
            if attempt < max_attempts - 1 {
                std::thread::sleep(retry_delay);
            }
        }

        Ok(None) // counting never completed within retry budget
    }

    pub fn create_issue(&self, create: &CreateIssue) -> Result<Issue> {
        self.post_json(
            &format!(
                "{}/api/issues?fields={}",
                self.base_url, DEFAULT_ISSUE_FIELDS
            ),
            create,
        )
    }

    pub fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        // When the update contains only State field changes, use the commands API.
        // Direct POST to /api/issues/{id} with state custom fields fails on some
        // YouTrack instances with HTTP 500 when the field name doesn't match exactly.
        // The commands API is more lenient and is what YouTrack's own UI uses.
        let only_state_changes = update.summary.is_none()
            && update.description.is_none()
            && update.tags.is_empty()
            && !update.custom_fields.is_empty()
            && update
                .custom_fields
                .iter()
                .all(|f| matches!(f, CustomFieldUpdate::State { .. }));

        if only_state_changes {
            for field in &update.custom_fields {
                if let CustomFieldUpdate::State {
                    name,
                    value: Some(v),
                } = field
                {
                    let query = format!("{}: {}", name, v.name);
                    self.apply_command(&[id], &query)?;
                }
            }
            return self.get_issue(id);
        }

        self.post_json(
            &format!(
                "{}/api/issues/{}?fields={}",
                self.base_url, id, DEFAULT_ISSUE_FIELDS
            ),
            update,
        )
    }

    /// Apply a YouTrack command to one or more issues via the Commands API.
    ///
    /// This uses `POST /api/commands` which accepts human-readable command strings
    /// (e.g. `"Stage: Done"`) and is more reliable than direct field POSTs for
    /// state transitions.
    fn apply_command(&self, issue_ids: &[&str], query: &str) -> Result<()> {
        let url = format!("{}/api/commands", self.base_url);

        let body = serde_json::json!({
            "query": query,
            "issues": issue_ids.iter().map(|id| serde_json::json!({"idReadable": id})).collect::<Vec<_>>()
        });

        self.post(&url, &body)?;
        Ok(())
    }

    pub fn delete_issue(&self, id: &str) -> Result<()> {
        self.delete(&format!("{}/api/issues/{}", self.base_url, id))
    }

    /// List attachments on an issue.
    pub fn list_issue_attachments(&self, issue_id: &str) -> Result<Vec<IssueAttachment>> {
        self.get_json(&format!(
            "{}/api/issues/{}/attachments?fields={}",
            self.base_url, issue_id, ISSUE_ATTACHMENT_FIELDS
        ))
    }

    /// Upload attachments to an issue.
    pub fn add_issue_attachments(
        &self,
        issue_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<IssueAttachment>> {
        if upload.comment.is_some() {
            return Err(YouTrackError::Api {
                status: 0,
                message: "YouTrack issue attachment upload does not support --comment".to_string(),
            });
        }

        let url = format!(
            "{}/api/issues/{}/attachments?fields={}",
            self.base_url, issue_id, ISSUE_ATTACHMENT_FIELDS
        );
        self.post_attachment_form(&url, upload)
    }

    /// Upload attachments to an issue comment.
    pub fn add_issue_comment_attachments(
        &self,
        issue_id: &str,
        comment_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<IssueAttachment>> {
        if upload.comment.is_some() {
            return Err(YouTrackError::Api {
                status: 0,
                message: "YouTrack issue comment attachment upload does not support --comment"
                    .to_string(),
            });
        }

        let url = format!(
            "{}/api/issues/{}/comments/{}/attachments?fields={}",
            self.base_url, issue_id, comment_id, ISSUE_ATTACHMENT_FIELDS
        );
        self.post_attachment_form(&url, upload)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        self.get_json(&format!(
            "{}/api/admin/projects?fields={}",
            self.base_url, DEFAULT_PROJECT_FIELDS
        ))
    }

    pub fn get_project(&self, id: &str) -> Result<Project> {
        self.get_json(&format!(
            "{}/api/admin/projects/{}?fields={}",
            self.base_url, id, DEFAULT_PROJECT_FIELDS
        ))
    }

    /// Get the currently authenticated user details.
    pub fn get_current_user(&self) -> Result<User> {
        self.get_json(&format!(
            "{}/api/users/me?fields={}",
            self.base_url, USER_FIELDS
        ))
    }

    /// Resolve a user identifier (database ID, login, or "me") to a YouTrack database user ID.
    pub fn resolve_user_id(&self, identifier: &str) -> Result<String> {
        let trimmed = identifier.trim();
        if trimmed.eq_ignore_ascii_case("me") {
            return self.get_current_user().map(|u| u.id);
        }

        // 1. Search users by query, matching login or database ID exactly
        let search_url = format!(
            "{}/api/users?query={}&fields={}&$top=50",
            self.base_url,
            urlencoding::encode(trimmed),
            USER_FIELDS
        );
        let users: Option<Vec<User>> = self.get_json_opt(&search_url)?;

        if let Some(user) = users.as_ref().and_then(|found| {
            found
                .iter()
                .find(|u| unicode_eq_ignore_case(&u.login, trimmed) || u.id == trimmed)
        }) {
            return Ok(user.id.clone());
        }

        // 2. Fall back to a direct lookup, in case the identifier is a database ID
        let direct_url = format!(
            "{}/api/users/{}?fields={}",
            self.base_url,
            urlencoding::encode(trimmed),
            USER_FIELDS
        );
        if let Some(user) = self.get_json_opt::<User>(&direct_url)? {
            return Ok(user.id);
        }

        let candidates: Vec<String> = users
            .unwrap_or_default()
            .into_iter()
            .map(|u| u.login)
            .take(5)
            .collect();
        let message = if candidates.is_empty() {
            format!(
                "Project leader '{}' could not be resolved to a YouTrack user",
                identifier
            )
        } else {
            format!(
                "Project leader '{}' could not be resolved to an exact user match. Candidate matches found: {}",
                identifier,
                candidates.join(", ")
            )
        };

        Err(YouTrackError::Api {
            status: 404,
            message,
        })
    }

    pub fn create_project(&self, create: &CreateProject) -> Result<Project> {
        self.post_json(
            &format!(
                "{}/api/admin/projects?fields={}",
                self.base_url, DEFAULT_PROJECT_FIELDS
            ),
            create,
        )
    }

    /// Resolve a project identifier (shortName or ID) to internal ID.
    /// If the input looks like an internal ID (contains '-' and starts with digit), returns it as-is.
    /// Otherwise, searches projects by shortName.
    pub fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        // If it looks like an internal ID (e.g., "0-2"), return as-is
        if identifier.contains('-')
            && identifier
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        {
            return Ok(identifier.to_string());
        }

        // Otherwise, search by shortName
        let projects = self.list_projects()?;
        for project in projects {
            if unicode_eq_ignore_case(&project.short_name, identifier) {
                return Ok(project.id);
            }
        }

        Err(YouTrackError::ProjectNotFound(identifier.to_string()))
    }

    /// Get custom fields defined for a project (with bundle values for enum fields)
    /// For state fields, also fetches isResolved and ordinal for workflow hints
    pub fn get_project_custom_fields(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectCustomFieldExt>> {
        self.get_json(&format!(
            "{}/api/admin/projects/{}/customFields?fields=id,canBeEmpty,emptyFieldText,field(id,name,fieldType(id,presentation)),bundle(id,values(id,name,isResolved,ordinal))",
            self.base_url, project_id
        ))
    }

    /// List users that can be assigned to issues in a project
    pub fn list_project_users(&self, project_id: &str) -> Result<Vec<User>> {
        // Response contains a team object with users array
        #[derive(serde::Deserialize)]
        struct TeamResponse {
            #[serde(default)]
            users: Vec<User>,
        }

        let team: TeamResponse = self.get_json(&format!(
            "{}/api/admin/projects/{}/team?fields=users(id,login,fullName,email)",
            self.base_url, project_id
        ))?;
        Ok(team.users)
    }

    /// List all available issue tags
    pub fn list_tags(&self) -> Result<Vec<IssueTag>> {
        // `issues(id)` is fetched only to count each tag's issues.
        self.get_json(&format!(
            "{}/api/issueTags?fields={},issues(id)",
            self.base_url, TAG_FIELDS
        ))
    }

    /// Create a new issue tag
    pub fn create_tag(&self, tag: &CreateIssueTagRequest) -> Result<IssueTag> {
        self.post_json(
            &format!("{}/api/issueTags?fields={}", self.base_url, TAG_FIELDS),
            tag,
        )
    }

    /// Delete an issue tag by ID
    pub fn delete_tag(&self, tag_id: &str) -> Result<()> {
        self.delete(&format!("{}/api/issueTags/{}", self.base_url, tag_id))
    }

    /// Update an existing issue tag by ID
    pub fn update_tag(&self, tag_id: &str, tag: &CreateIssueTagRequest) -> Result<IssueTag> {
        self.post_json(
            &format!(
                "{}/api/issueTags/{}?fields={}",
                self.base_url, tag_id, TAG_FIELDS
            ),
            tag,
        )
    }

    /// List all available issue link types
    pub fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        self.get_json(&format!(
            "{}/api/issueLinkTypes?fields=id,name,sourceToTarget,targetToSource,directed",
            self.base_url
        ))
    }

    /// Get links for an issue
    pub fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        self.get_json(&format!(
            "{}/api/issues/{}/links?fields=id,direction,linkType(id,name,sourceToTarget,targetToSource,directed),issues(id,idReadable,summary)",
            self.base_url, issue_id
        ))
    }

    /// Add target issue to a link type for source issue
    /// `source_issue_id` is the issue to add the link from
    /// `link_id` is the link type id with direction suffix (e.g., "142-3t" for inward subtask)
    /// `target_issue_id` is the issue to link to
    pub fn add_issue_to_link(
        &self,
        source_issue_id: &str,
        link_id: &str,
        target_issue_id: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/api/issues/{}/links/{}/issues",
            self.base_url, source_issue_id, link_id
        );

        let body = IssueRef {
            id_readable: target_issue_id.to_string(),
        };

        self.post(&url, &body)?;
        Ok(())
    }

    /// Find the link ID for a given link type name and direction
    /// Returns the link ID that can be used with add_issue_to_link
    fn find_link_id(
        &self,
        issue_id: &str,
        link_type_name: &str,
        direction: &str,
    ) -> Result<String> {
        let links = self.get_issue_links(issue_id)?;

        for link in links {
            if unicode_eq_ignore_case(&link.link_type.name, link_type_name)
                && let Some(dir) = &link.direction
                && dir.eq_ignore_ascii_case(direction)
            {
                return Ok(link.id);
            }
        }

        Err(YouTrackError::Api {
            status: 404,
            message: format!(
                "Link type '{}' with direction '{}' not found for issue '{}'",
                link_type_name, direction, issue_id
            ),
        })
    }

    /// Create a subtask link (child issue is subtask of parent issue)
    /// This creates the link from the child's perspective using "Subtask" link type
    pub fn link_subtask(&self, child_issue_id: &str, parent_issue_id: &str) -> Result<()> {
        // Find the INWARD subtask link ID for the child issue
        // INWARD means "subtask of" (child is subtask of parent)
        let link_id = self.find_link_id(child_issue_id, "Subtask", "INWARD")?;

        self.add_issue_to_link(child_issue_id, &link_id, parent_issue_id)
    }

    /// Remove a target issue from a link bucket.
    ///
    /// `source_issue_id` is the issue that owns the link.
    /// `bucket_id` is the YouTrack link bucket ID (e.g., `"142-3t"`).
    /// `target_issue_id` is the target issue readable ID to remove (e.g., `"PROJ-456"`).
    pub fn remove_issue_from_link(
        &self,
        source_issue_id: &str,
        bucket_id: &str,
        target_issue_id: &str,
    ) -> Result<()> {
        self.delete(&format!(
            "{}/api/issues/{}/links/{}/issues/{}",
            self.base_url, source_issue_id, bucket_id, target_issue_id
        ))
    }

    /// Link two issues together with the specified link type.
    /// `link_type` is a canonical name (e.g. "relates", "depends") that gets
    /// resolved to the YouTrack-native name via `resolve_link_type`.
    /// `direction` should be "OUTWARD", "INWARD", or "BOTH" depending on the link type.
    pub fn link_issues(
        &self,
        source_issue_id: &str,
        target_issue_id: &str,
        link_type: &str,
        direction: &str,
    ) -> Result<()> {
        let resolved_type = self.resolve_link_type(link_type);
        let link_id = self.find_link_id(source_issue_id, &resolved_type, direction)?;
        self.add_issue_to_link(source_issue_id, &link_id, target_issue_id)
    }

    /// Add a comment to an issue
    pub fn add_comment(&self, issue_id: &str, text: &str) -> Result<IssueComment> {
        let url = format!(
            "{}/api/issues/{}/comments?fields={}",
            self.base_url, issue_id, COMMENT_FIELDS
        );

        let comment = CreateComment {
            text: text.to_string(),
        };

        self.post_json(&url, &comment)
    }

    /// Get comments for an issue
    pub fn get_comments(&self, issue_id: &str) -> Result<Vec<IssueComment>> {
        self.get_comments_page(issue_id, 100, 0)
    }

    /// Get a page of comments for an issue
    pub fn get_comments_page(
        &self,
        issue_id: &str,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<IssueComment>> {
        self.get_json(&format!(
            "{}/api/issues/{}/comments?fields={}&$top={}&$skip={}",
            self.base_url, issue_id, COMMENT_FIELDS, limit, skip
        ))
    }

    // ========================================================================
    // History Operations
    // ========================================================================

    /// Fetch one page of an issue's custom-field activity timeline.
    ///
    /// `GET /api/issues/{id}/activities` is offset-paged (`$top`/`$skip`). We
    /// scope to `CustomFieldCategory` so only field transitions come back
    /// (comments/work-items have their own commands). Activities are
    /// oldest-first; the caller re-sorts.
    pub fn get_activities_page(
        &self,
        issue_id: &str,
        top: usize,
        skip: usize,
    ) -> Result<Vec<YouTrackActivity>> {
        self.get_json(&format!(
            "{}/api/issues/{}/activities?categories=CustomFieldCategory&fields=id,timestamp,author(login,name),field(name,presentation),added(name,text,login,presentation),removed(name,text,login,presentation)&$top={}&$skip={}",
            self.base_url, issue_id, top, skip
        ))
    }

    /// Fetch an issue's complete custom-field activity timeline, paging through
    /// every entry. Activities are returned oldest-first (the caller re-sorts).
    ///
    /// Unlike Jira's changelog endpoint, YouTrack honors `$top`, so a page
    /// shorter than requested is a reliable end-of-data signal. A generous
    /// `MAX_PAGES` backstop guards against a server that ignores `$skip` (which
    /// would otherwise loop forever) without truncating a legitimately large
    /// history.
    pub fn get_issue_activities(&self, issue_id: &str) -> Result<Vec<YouTrackActivity>> {
        const PAGE: usize = 100;
        const MAX_PAGES: usize = 10_000;
        let mut activities = Vec::new();
        let mut skip = 0;
        for _ in 0..MAX_PAGES {
            let page = self.get_activities_page(issue_id, PAGE, skip)?;
            let fetched = page.len();
            activities.extend(page);
            // YouTrack honors `$top`, so a short (or empty) page means we have
            // reached the end of the timeline.
            if fetched < PAGE {
                return Ok(activities);
            }
            skip += fetched;
        }
        Err(YouTrackError::PaginationStalled(format!(
            "issue '{}' activity timeline did not terminate after {} pages",
            issue_id, MAX_PAGES
        )))
    }

    // ========================================================================
    // Knowledge Base / Article Operations
    // ========================================================================

    /// Get an article by ID (database ID or readable ID like PROJ-A-1)
    pub fn get_article(&self, id: &str) -> Result<Article> {
        self.get_json(&format!(
            "{}/api/articles/{}?fields={}",
            self.base_url, id, DEFAULT_ARTICLE_FIELDS
        ))
    }

    /// List articles with pagination
    pub fn list_articles(&self, limit: usize, skip: usize) -> Result<Vec<Article>> {
        self.get_json(&format!(
            "{}/api/articles?fields={}&$top={}&$skip={}",
            self.base_url, DEFAULT_ARTICLE_FIELDS, limit, skip
        ))
    }

    /// Search articles using YouTrack query language
    pub fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>> {
        self.get_json(&format!(
            "{}/api/articles?query={}&fields={}&$top={}&$skip={}",
            self.base_url,
            urlencoding::encode(query),
            DEFAULT_ARTICLE_FIELDS,
            limit,
            skip
        ))
    }

    /// Create a new article
    pub fn create_article(&self, create: &CreateArticle) -> Result<Article> {
        self.post_json(
            &format!(
                "{}/api/articles?fields={}",
                self.base_url, DEFAULT_ARTICLE_FIELDS
            ),
            create,
        )
    }

    /// Update an existing article
    pub fn update_article(&self, id: &str, update: &UpdateArticle) -> Result<Article> {
        self.post_json(
            &format!(
                "{}/api/articles/{}?fields={}",
                self.base_url, id, DEFAULT_ARTICLE_FIELDS
            ),
            update,
        )
    }

    /// Delete an article
    pub fn delete_article(&self, id: &str) -> Result<()> {
        self.delete(&format!("{}/api/articles/{}", self.base_url, id))
    }

    /// Get child articles of a parent article
    pub fn get_child_articles(&self, parent_id: &str) -> Result<Vec<Article>> {
        self.get_json(&format!(
            "{}/api/articles/{}/childArticles?fields={}",
            self.base_url, parent_id, DEFAULT_ARTICLE_FIELDS
        ))
    }

    /// Move an article to a new parent (or to root if new_parent_id is None)
    /// This is done by updating the article's parentArticle field
    pub fn move_article(&self, article_id: &str, new_parent_id: Option<&str>) -> Result<Article> {
        let url = format!(
            "{}/api/articles/{}?fields={}",
            self.base_url, article_id, DEFAULT_ARTICLE_FIELDS
        );

        // `parentArticle` is always sent: an explicit `null` is what moves the
        // article to the root, whereas omitting it leaves the parent unchanged.
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct MoveArticle {
            parent_article: Option<ArticleIdentifier>,
        }

        let body = MoveArticle {
            parent_article: new_parent_id.map(|id| ArticleIdentifier { id: id.to_string() }),
        };

        self.post_json(&url, &body)
    }

    /// List attachments on an article
    pub fn list_article_attachments(&self, article_id: &str) -> Result<Vec<ArticleAttachment>> {
        self.get_json(&format!(
            "{}/api/articles/{}/attachments?fields={}",
            self.base_url, article_id, ARTICLE_ATTACHMENT_FIELDS
        ))
    }

    /// Upload attachments to an article.
    pub fn add_article_attachments(
        &self,
        article_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<ArticleAttachment>> {
        if upload.comment.is_some() {
            return Err(YouTrackError::Api {
                status: 0,
                message: "YouTrack article attachment upload does not support --comment"
                    .to_string(),
            });
        }
        if upload.minor_edit {
            return Err(YouTrackError::Api {
                status: 0,
                message: "YouTrack article attachment upload does not support --minor-edit"
                    .to_string(),
            });
        }

        let url = format!(
            "{}/api/articles/{}/attachments?fields={}",
            self.base_url, article_id, ARTICLE_ATTACHMENT_FIELDS
        );
        self.post_attachment_form(&url, upload)
    }

    /// Upload attachments to an article comment.
    pub fn add_article_comment_attachments(
        &self,
        article_id: &str,
        comment_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<ArticleAttachment>> {
        if upload.comment.is_some() {
            return Err(YouTrackError::Api {
                status: 0,
                message: "YouTrack article comment attachment upload does not support --comment"
                    .to_string(),
            });
        }
        if upload.minor_edit {
            return Err(YouTrackError::Api {
                status: 0,
                message: "YouTrack article comment attachment upload does not support --minor-edit"
                    .to_string(),
            });
        }

        let url = format!(
            "{}/api/articles/{}/comments/{}/attachments?fields={}",
            self.base_url, article_id, comment_id, ARTICLE_ATTACHMENT_FIELDS
        );
        self.post_attachment_form(&url, upload)
    }

    /// Get comments on an article
    pub fn get_article_comments(&self, article_id: &str) -> Result<Vec<ArticleComment>> {
        self.get_json(&format!(
            "{}/api/articles/{}/comments?fields={}",
            self.base_url, article_id, COMMENT_FIELDS
        ))
    }

    /// Add a comment to an article
    pub fn add_article_comment(&self, article_id: &str, text: &str) -> Result<ArticleComment> {
        let url = format!(
            "{}/api/articles/{}/comments?fields={}",
            self.base_url, article_id, COMMENT_FIELDS
        );

        let comment = CreateArticleComment {
            text: text.to_string(),
        };

        self.post_json(&url, &comment)
    }

    // ========================================================================
    // Custom Field Admin Operations
    // ========================================================================

    const DEFAULT_CUSTOM_FIELD_FIELDS: &'static str = "id,name,fieldType(id,presentation)";
    const DEFAULT_BUNDLE_FIELDS: &'static str =
        "id,name,$type,values(id,name,description,isResolved,ordinal)";

    /// List all custom field definitions (instance-wide)
    pub fn list_custom_field_definitions(&self) -> Result<Vec<CustomFieldResponse>> {
        self.get_json(&format!(
            "{}/api/admin/customFieldSettings/customFields?fields={}",
            self.base_url,
            Self::DEFAULT_CUSTOM_FIELD_FIELDS
        ))
    }

    /// Create a new custom field definition
    pub fn create_custom_field(
        &self,
        create: &CreateCustomFieldRequest,
    ) -> Result<CustomFieldResponse> {
        self.post_json(
            &format!(
                "{}/api/admin/customFieldSettings/customFields?fields={}",
                self.base_url,
                Self::DEFAULT_CUSTOM_FIELD_FIELDS
            ),
            create,
        )
    }

    /// List bundles of a given type (enum, state, etc.)
    pub fn list_bundles(&self, bundle_type: &str) -> Result<Vec<BundleResponse>> {
        self.get_json(&format!(
            "{}/api/admin/customFieldSettings/bundles/{}?fields={}",
            self.base_url,
            bundle_type,
            Self::DEFAULT_BUNDLE_FIELDS
        ))
    }

    /// Create a new bundle
    pub fn create_bundle(
        &self,
        bundle_type: &str,
        create: &CreateBundleRequest,
    ) -> Result<BundleResponse> {
        self.post_json(
            &format!(
                "{}/api/admin/customFieldSettings/bundles/{}?fields={}",
                self.base_url,
                bundle_type,
                Self::DEFAULT_BUNDLE_FIELDS
            ),
            create,
        )
    }

    /// Add a value to an existing bundle
    pub fn add_bundle_value(
        &self,
        bundle_type: &str,
        bundle_id: &str,
        value: &CreateBundleValueRequest,
    ) -> Result<BundleValueResponse> {
        self.post_json(
            &format!(
                "{}/api/admin/customFieldSettings/bundles/{}/{}/values?fields=id,name,description,isResolved,ordinal",
                self.base_url, bundle_type, bundle_id
            ),
            value,
        )
    }

    /// Attach a custom field to a project
    pub fn attach_field_to_project(
        &self,
        project_id: &str,
        attach: &AttachFieldRequest,
    ) -> Result<ProjectCustomFieldResponse> {
        self.post_json(
            &format!(
                "{}/api/admin/projects/{}/customFields?fields=id,field(id,name,fieldType(id,presentation)),bundle(id,values(id,name,isResolved,ordinal)),canBeEmpty,emptyFieldText",
                self.base_url, project_id
            ),
            attach,
        )
    }
}
