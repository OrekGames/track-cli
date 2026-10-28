use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tracker_core::AttachmentUpload;
use ureq::http::Response;
use ureq::typestate::{WithBody, WithoutBody};
use ureq::unversioned::multipart::{Form, Part};
use ureq::{Agent, Body, RequestBuilder};

use crate::error::{JiraError, Result};
use crate::models::*;

/// Jira REST API client
pub struct JiraClient {
    agent: Agent,
    base_url: String,
    auth_header: String,
    link_mappings: HashMap<String, String>,
    /// Cached instance-level field metadata (lazy-loaded)
    pub(crate) field_cache: Mutex<Option<Vec<JiraField>>>,
}

impl JiraClient {
    /// Create a new Jira client with Basic Auth
    ///
    /// For Jira Cloud, use your email and an API token.
    /// For Jira Server, use your username and password.
    pub fn new(base_url: &str, email: &str, api_token: &str) -> Self {
        Self {
            agent: http_agent(),
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_header: basic_auth_header(email, api_token),
            link_mappings: HashMap::new(),
            field_cache: Mutex::new(None),
        }
    }

    /// Set custom link type mappings (canonical name -> Jira link type name)
    pub fn with_link_mappings(mut self, mappings: HashMap<String, String>) -> Self {
        self.link_mappings = mappings;
        self
    }

    /// Resolve a canonical link type name to the Jira-native link type name.
    /// User overrides take precedence, then built-in defaults, then pass-through.
    pub(crate) fn resolve_link_type(&self, canonical: &str) -> String {
        if let Some(name) = self.link_mappings.get(canonical) {
            return name.clone();
        }
        match canonical {
            "relates" => "Relates",
            "depends" => "Blocks",
            "required" => "Blocks",
            "duplicates" => "Duplicate",
            "duplicated-by" => "Duplicate",
            _ => canonical,
        }
        .to_string()
    }

    /// Build a Jira REST API v3 URL for `path`.
    fn api_url(&self, path: &str) -> String {
        format!("{}/rest/api/3{}", self.base_url, path)
    }

    /// Add the authentication and JSON `Accept` headers every request carries.
    fn authorized<B>(&self, request: RequestBuilder<B>) -> RequestBuilder<B> {
        request
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
    }

    /// Send a request without a body (GET, DELETE); error on a non-2xx status.
    fn call(&self, request: RequestBuilder<WithoutBody>) -> Result<Response<Body>> {
        self.check_response(self.authorized(request).call()?)
    }

    /// Send `body` as JSON (POST, PUT); error on a non-2xx status.
    fn send_json(
        &self,
        request: RequestBuilder<WithBody>,
        body: &impl Serialize,
    ) -> Result<Response<Body>> {
        let response = self
            .authorized(request)
            .header("Content-Type", "application/json")
            .send_json(body)?;
        self.check_response(response)
    }

    /// GET `url` and deserialize its JSON response.
    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        read_json(self.call(self.agent.get(url))?)
    }

    /// Check response status and return error if not successful
    fn check_response(&self, mut response: Response<Body>) -> Result<Response<Body>> {
        let status = response.status().as_u16();

        if (200..300).contains(&status) {
            return Ok(response);
        }

        // Try to read error body for better error messages
        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| String::new());

        // Try to parse as Jira error response
        let message = if let Ok(error_response) = serde_json::from_str::<serde_json::Value>(&body) {
            // Jira error format: {"errorMessages":["..."], "errors":{...}}
            let mut messages = Vec::new();

            if let Some(errors) = error_response
                .get("errorMessages")
                .and_then(|e| e.as_array())
            {
                for e in errors {
                    if let Some(s) = e.as_str() {
                        messages.push(s.to_string());
                    }
                }
            }

            if let Some(errors) = error_response.get("errors").and_then(|e| e.as_object()) {
                for (field, msg) in errors {
                    if let Some(s) = msg.as_str() {
                        messages.push(format!("{}: {}", field, s));
                    }
                }
            }

            if messages.is_empty() {
                body
            } else {
                messages.join("; ")
            }
        } else if body.is_empty() {
            format!("HTTP {}", status)
        } else {
            body
        };

        if status == 401 {
            Err(JiraError::Unauthorized)
        } else {
            Err(JiraError::Api { status, message })
        }
    }

    // ==================== Issue Operations ====================

    /// Get an issue by key or ID
    pub fn get_issue(&self, key: &str) -> Result<JiraIssue> {
        // `fields=*all` mirrors the search endpoint so a single-issue read
        // surfaces the same set of (system + custom) fields, keeping the
        // lossless projection consistent between `get` and `search`.
        self.get_json(&self.api_url(&format!("/issue/{}?fields=*all", key)))
    }

    /// Search issues using JQL.
    ///
    /// `/search/jql` is cursor-based: pass `next_page_token = None` for the
    /// first page, then the token from the previous response's
    /// `next_page_token` for each subsequent page. The endpoint does not
    /// support offset pagination (`startAt` is silently ignored), and the
    /// server may cap or shrink pages below `max_results`.
    pub fn search_issues(
        &self,
        jql: &str,
        max_results: usize,
        next_page_token: Option<&str>,
    ) -> Result<JiraSearchResult> {
        // Jira Cloud uses GET /search/jql (the old POST /search was removed).
        // `fields=*all` is required — without it the endpoint returns only issue IDs.
        let jql_encoded = urlencoding::encode(jql);
        let mut url = format!(
            "{}?jql={}&maxResults={}&fields=*all",
            self.api_url("/search/jql"),
            jql_encoded,
            max_results,
        );
        if let Some(token) = next_page_token {
            // The token is an opaque server string; percent-encode it.
            url.push_str("&nextPageToken=");
            url.push_str(&urlencoding::encode(token));
        }

        self.get_json(&url)
    }

    /// Create a new issue
    pub fn create_issue(&self, issue: &CreateJiraIssue) -> Result<JiraIssue> {
        let url = self.api_url("/issue");
        let response = self.send_json(self.agent.post(&url), issue)?;

        // Jira returns a minimal response on create, we need to fetch the full issue
        #[derive(serde::Deserialize)]
        struct CreateResponse {
            key: String,
        }
        let created: CreateResponse = read_json(response)?;

        // Fetch the full issue
        self.get_issue(&created.key)
    }

    /// Update an existing issue
    pub fn update_issue(&self, key: &str, update: &UpdateJiraIssue) -> Result<JiraIssue> {
        let url = self.api_url(&format!("/issue/{}", key));
        self.send_json(self.agent.put(&url), update)?;

        // Jira returns 204 No Content on update, fetch the updated issue
        self.get_issue(key)
    }

    /// Delete an issue
    pub fn delete_issue(&self, key: &str) -> Result<()> {
        let url = self.api_url(&format!("/issue/{}", key));
        self.call(self.agent.delete(&url))?;
        Ok(())
    }

    /// Upload attachments to an issue.
    pub fn add_issue_attachments(
        &self,
        key: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<JiraAttachment>> {
        if upload.comment.is_some() {
            return Err(JiraError::Api {
                status: 0,
                message: "Jira issue attachment upload does not support --comment".to_string(),
            });
        }

        let url = self.api_url(&format!("/issue/{}/attachments", key));

        let mut form = Form::new();
        for file in &upload.files {
            let mut part = Part::file(&file.path)?;
            if let Some(name) = &file.name {
                part = part.file_name(name);
            }
            if let Some(mime_type) = &file.mime_type {
                part = part.mime_str(mime_type)?;
            }
            form = form.part("file", part);
        }

        let response = self
            .authorized(self.agent.post(&url))
            .header("X-Atlassian-Token", "no-check")
            .send(form)?;
        read_json(self.check_response(response)?)
    }

    // ==================== Project Operations ====================

    /// List all accessible projects
    pub fn list_projects(&self) -> Result<Vec<JiraProject>> {
        self.get_json(&self.api_url("/project"))
    }

    /// Get a project by key or ID
    pub fn get_project(&self, key: &str) -> Result<JiraProject> {
        self.get_json(&self.api_url(&format!("/project/{}", key)))
    }

    // ==================== Comment Operations ====================

    /// Add a comment to an issue
    pub fn add_comment(&self, key: &str, body: &str) -> Result<JiraComment> {
        let url = self.api_url(&format!("/issue/{}/comment", key));
        let comment = CreateJiraComment::from_text(body);
        read_json(self.send_json(self.agent.post(&url), &comment)?)
    }

    /// Get comments on an issue
    pub fn get_comments(&self, key: &str) -> Result<Vec<JiraComment>> {
        self.get_comments_page(key, 100, 0)
    }

    /// Get a page of comments on an issue
    pub fn get_comments_page(
        &self,
        key: &str,
        max_results: usize,
        start_at: usize,
    ) -> Result<Vec<JiraComment>> {
        let url = format!(
            "{}?startAt={}&maxResults={}",
            self.api_url(&format!("/issue/{}/comment", key)),
            start_at,
            max_results
        );
        let page: JiraCommentsResponse = self.get_json(&url)?;
        Ok(page.comments)
    }

    // ==================== History Operations ====================

    /// Fetch one page of an issue's changelog.
    ///
    /// `GET /issue/{key}/changelog` is offset-paged (`startAt`/`maxResults`),
    /// unlike the cursor-based issue search.
    pub fn get_issue_changelog_page(
        &self,
        key: &str,
        start_at: usize,
        max_results: usize,
    ) -> Result<JiraChangelogPage> {
        let url = format!(
            "{}?startAt={}&maxResults={}",
            self.api_url(&format!("/issue/{}/changelog", key)),
            start_at,
            max_results
        );
        self.get_json(&url)
    }

    /// Fetch an issue's complete changelog, paging through every entry.
    ///
    /// Jira returns changelog entries oldest-first; that order is preserved
    /// here (the caller re-sorts as needed).
    pub fn get_issue_changelog(&self, key: &str) -> Result<Vec<JiraChangelogEntry>> {
        const PAGE: usize = 100;
        // Generous backstop: real changelogs are orders of magnitude smaller.
        // It guards against a server that ignores `startAt` or never sets
        // `isLast` (which would otherwise loop forever) without silently
        // truncating a legitimately large history. We do NOT stop on a short
        // page — Jira may cap `maxResults` below what we asked, so a short page
        // is not a reliable end-of-data signal; `isLast`/`total` are.
        const MAX_PAGES: usize = 10_000;
        let mut entries = Vec::new();
        let mut start_at = 0;
        for _ in 0..MAX_PAGES {
            let page = self.get_issue_changelog_page(key, start_at, PAGE)?;
            let fetched = page.values.len();
            entries.extend(page.values);
            if page.is_last || fetched == 0 || (page.total > 0 && entries.len() >= page.total) {
                return Ok(entries);
            }
            start_at += fetched;
        }
        Err(JiraError::PaginationStalled(format!(
            "issue '{}' changelog did not terminate after {} pages",
            key, MAX_PAGES
        )))
    }

    // ==================== Link Operations ====================

    /// Create a link between two issues
    pub fn create_link(&self, link: &CreateJiraIssueLink) -> Result<()> {
        self.send_json(self.agent.post(&self.api_url("/issueLink")), link)?;
        Ok(())
    }

    /// Delete an issue link by its link ID
    pub fn delete_link(&self, link_id: &str) -> Result<()> {
        let url = self.api_url(&format!("/issueLink/{}", link_id));
        self.call(self.agent.delete(&url))?;
        Ok(())
    }

    // ==================== Field Operations ====================

    /// Get field metadata, using a cached copy if available.
    /// Falls back to an empty list on error (graceful degradation).
    pub fn get_fields_cached(&self) -> Vec<JiraField> {
        let mut cache = self.field_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref fields) = *cache {
            return fields.clone();
        }
        match self.get_all_fields() {
            Ok(fields) => {
                *cache = Some(fields.clone());
                fields
            }
            Err(_) => vec![],
        }
    }

    /// Fetch all field definitions from the Jira instance.
    /// Returns system fields and instance-level custom fields with their IDs, names, and types.
    pub fn get_all_fields(&self) -> Result<Vec<JiraField>> {
        self.get_json(&self.api_url("/field"))
    }

    // ==================== Label Operations ====================

    /// List all labels in the Jira instance
    pub fn list_labels(&self) -> Result<Vec<String>> {
        // Jira returns: {"total": N, "maxResults": N, "values": ["label1", "label2"]}
        #[derive(serde::Deserialize)]
        struct LabelResponse {
            values: Vec<String>,
        }
        let result: LabelResponse = self.get_json(&self.api_url("/label"))?;
        Ok(result.values)
    }

    /// List all issue link types
    pub fn list_link_types(&self) -> Result<Vec<JiraIssueLinkType>> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct LinkTypesResponse {
            issue_link_types: Vec<JiraIssueLinkType>,
        }

        let result: LinkTypesResponse = self.get_json(&self.api_url("/issueLinkType"))?;
        Ok(result.issue_link_types)
    }

    /// List users assignable to issues in a project
    pub fn list_assignable_users(&self, project_key: &str) -> Result<Vec<JiraUser>> {
        let url = format!(
            "{}?project={}",
            self.api_url("/user/assignable/search"),
            urlencoding::encode(project_key)
        );
        self.get_json(&url)
    }

    // ==================== Transition Operations ====================

    /// GET /rest/api/3/issue/{key}/transitions
    pub fn list_transitions(&self, issue_key: &str) -> Result<Vec<Transition>> {
        let url = self.api_url(&format!("/issue/{}/transitions", issue_key));
        let resp: TransitionsResponse = self.get_json(&url)?;
        Ok(resp.transitions)
    }

    /// POST /rest/api/3/issue/{key}/transitions
    pub fn transition_issue(&self, issue_key: &str, transition_id: &str) -> Result<()> {
        let url = self.api_url(&format!("/issue/{}/transitions", issue_key));
        let body = TransitionRequest {
            transition: TransitionId {
                id: transition_id.to_string(),
            },
        };
        self.send_json(self.agent.post(&url), &body)?;
        Ok(())
    }

    /// Resolve a user-provided status name (case-insensitive) to a transition id
    /// available on the issue's current workflow step.
    pub fn resolve_transition_id(&self, issue_key: &str, target_status: &str) -> Result<String> {
        let target = target_status.trim();
        let target_is_ascii = target.is_ascii();
        let transitions = self.list_transitions(issue_key)?;

        let mut target_lower: Option<String> = None;

        // Helper to check case-insensitive match without unnecessary allocations
        // when both strings are ASCII, falling back to Unicode lowercasing otherwise.
        let mut is_match = |name: &str| {
            if target_is_ascii && name.is_ascii() {
                name.eq_ignore_ascii_case(target)
            } else {
                let tl = target_lower.get_or_insert_with(|| target.to_lowercase());
                &name.to_lowercase() == tl
            }
        };

        // Prefer exact match on the target status name, fall back to transition name.
        if let Some(t) = transitions.iter().find(|t| is_match(&t.to.name)) {
            return Ok(t.id.clone());
        }
        if let Some(t) = transitions.iter().find(|t| is_match(&t.name)) {
            return Ok(t.id.clone());
        }

        let available: Vec<String> = transitions.iter().map(|t| t.to.name.clone()).collect();
        Err(JiraError::InvalidTransition {
            requested: target_status.to_string(),
            available,
        })
    }

    // ==================== Project Status Operations ====================

    /// GET /rest/api/3/project/{projectIdOrKey}/statuses
    pub fn list_project_statuses(
        &self,
        project_key: &str,
    ) -> Result<Vec<ProjectIssueTypeStatuses>> {
        self.get_json(&self.api_url(&format!("/project/{}/statuses", project_key)))
    }

    /// GET /rest/api/3/project/{projectIdOrKey}/components
    pub fn list_project_components(&self, project_key: &str) -> Result<Vec<JiraComponent>> {
        self.get_json(&self.api_url(&format!("/project/{}/components", project_key)))
    }
}

/// HTTP agent shared by the Jira and Confluence clients. Non-2xx responses are
/// returned rather than raised, so the clients can surface Atlassian's error body.
pub(crate) fn http_agent() -> Agent {
    Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// `Authorization` header value for Atlassian Basic Auth (email + API token).
pub(crate) fn basic_auth_header(email: &str, api_token: &str) -> String {
    let credentials = format!("{}:{}", email.trim(), api_token.trim());
    format!("Basic {}", base64_encode(&credentials))
}

/// Deserialize a JSON response body.
pub(crate) fn read_json<T: DeserializeOwned>(mut response: Response<Body>) -> Result<T> {
    Ok(response.body_mut().read_json()?)
}

/// Simple base64 encoding function
fn base64_encode(input: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let bytes = input.as_bytes();
    let mut result = String::new();

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }

    result
}
