use std::collections::HashMap;
use std::time::Duration;
use ureq::Agent;

use crate::error::{GitLabError, Result};
use crate::models::*;

/// GitLab REST API client
pub struct GitLabClient {
    agent: Agent,
    base_url: String,
    token: String,
    project_id: Option<String>,
    link_mappings: HashMap<String, String>,
}

impl GitLabClient {
    /// Create a new GitLab client.
    ///
    /// `base_url` should include the API version path, e.g. `https://gitlab.com/api/v4`.
    /// `project_id` can be a numeric ID or a URL-encoded path like `group%2Fproject`.
    pub fn new(base_url: &str, token: &str, project_id: Option<&str>) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();

        Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            project_id: project_id.map(|s| s.to_string()),
            link_mappings: HashMap::new(),
        }
    }

    /// Set custom link type mappings (canonical name -> GitLab link type string)
    pub fn with_link_mappings(mut self, mappings: HashMap<String, String>) -> Self {
        self.link_mappings = mappings;
        self
    }

    /// Resolve a canonical link type name to the GitLab-native link type string.
    /// User overrides take precedence, then built-in defaults, then pass-through.
    pub(crate) fn resolve_link_type(&self, canonical: &str) -> String {
        if let Some(name) = self.link_mappings.get(canonical) {
            return name.clone();
        }
        match canonical {
            "relates" => "relates_to",
            "depends" => "blocks",
            "required" => "is_blocked_by",
            "duplicates" => "relates_to",
            "duplicated-by" => "relates_to",
            _ => canonical,
        }
        .to_string()
    }

    /// Get the project ID string (or "unknown" if not configured).
    /// Used by trait_impl for building core models.
    pub(crate) fn project_id_str(&self) -> String {
        self.project_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Build a project-scoped URL. Returns an error if project_id is not configured.
    fn project_url(&self, path: &str) -> Result<String> {
        let project_id = self.project_id.as_ref().ok_or_else(|| GitLabError::Api {
            status: 0,
            message: "GitLab project_id is not configured. Set via 'track config set gitlab.project_id <ID>' or GITLAB_PROJECT_ID env var".to_string(),
        })?;
        Ok(format!(
            "{}/projects/{}{}",
            self.base_url,
            urlencoding::encode(project_id),
            path
        ))
    }

    /// Check response status and return error if not successful
    fn check_response(
        &self,
        mut response: ureq::http::Response<ureq::Body>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let status = response.status().as_u16();

        if (200..300).contains(&status) {
            return Ok(response);
        }

        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| String::new());

        // Try to parse as GitLab error JSON
        let message = if let Ok(error_value) = serde_json::from_str::<serde_json::Value>(&body) {
            // GitLab can return {"message": "..."} or {"error": "..."}
            if let Some(msg) = error_value.get("message").and_then(|m| m.as_str()) {
                msg.to_string()
            } else if let Some(msg) = error_value.get("error").and_then(|e| e.as_str()) {
                msg.to_string()
            } else if let Some(msg) = error_value
                .get("error_description")
                .and_then(|e| e.as_str())
            {
                msg.to_string()
            } else if body.is_empty() {
                format!("HTTP {}", status)
            } else {
                body
            }
        } else if body.is_empty() {
            format!("HTTP {}", status)
        } else {
            body
        };

        if status == 401 {
            Err(GitLabError::Unauthorized)
        } else {
            Err(GitLabError::Api { status, message })
        }
    }

    /// Handle transport-level errors
    fn handle_error(&self, err: ureq::Error) -> GitLabError {
        GitLabError::Http(err)
    }

    // ==================== Issue Operations ====================

    /// Get an issue by IID (project-scoped number)
    pub fn get_issue(&self, iid: u64) -> Result<GitLabIssue> {
        let url = self.project_url(&format!("/issues/{}", iid))?;

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let issue: GitLabIssue = response.body_mut().read_json()?;
        Ok(issue)
    }

    /// List issues for the project
    pub fn list_issues(
        &self,
        state: Option<&str>,
        per_page: usize,
        page: usize,
    ) -> Result<Vec<GitLabIssue>> {
        let (issues, _total) = self.list_issues_with_total(state, per_page, page)?;
        Ok(issues)
    }

    /// List issues and also return the X-Total header count (if present).
    /// This avoids a separate count API call.
    pub fn list_issues_with_total(
        &self,
        state: Option<&str>,
        per_page: usize,
        page: usize,
    ) -> Result<(Vec<GitLabIssue>, Option<u64>)> {
        let mut url = self.project_url(&format!("/issues?per_page={}&page={}", per_page, page))?;
        if let Some(s) = state {
            url.push_str(&format!("&state={}", urlencoding::encode(s)));
        }

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let total = response
            .headers()
            .get("x-total")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        let mut response = self.check_response(response)?;
        let issues: Vec<GitLabIssue> = response.body_mut().read_json()?;
        Ok((issues, total))
    }

    /// Search issues with query text, state, and labels
    pub fn search_issues(
        &self,
        search: &str,
        state: Option<&str>,
        labels: Option<&str>,
        per_page: usize,
        page: usize,
    ) -> Result<Vec<GitLabIssue>> {
        let (issues, _total) =
            self.search_issues_with_total(search, state, labels, per_page, page)?;
        Ok(issues)
    }

    /// Search issues and also return the X-Total header count (if present).
    /// This avoids a separate count API call.
    pub fn search_issues_with_total(
        &self,
        search: &str,
        state: Option<&str>,
        labels: Option<&str>,
        per_page: usize,
        page: usize,
    ) -> Result<(Vec<GitLabIssue>, Option<u64>)> {
        let mut url = self.project_url(&format!(
            "/issues?search={}&per_page={}&page={}",
            urlencoding::encode(search),
            per_page,
            page
        ))?;
        if let Some(s) = state {
            url.push_str(&format!("&state={}", urlencoding::encode(s)));
        }
        if let Some(l) = labels {
            url.push_str(&format!("&labels={}", urlencoding::encode(l)));
        }

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let total = response
            .headers()
            .get("x-total")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        let mut response = self.check_response(response)?;
        let issues: Vec<GitLabIssue> = response.body_mut().read_json()?;
        Ok((issues, total))
    }

    /// Create a new issue
    pub fn create_issue(&self, issue: &CreateGitLabIssue) -> Result<GitLabIssue> {
        let url = self.project_url("/issues")?;

        let response = self
            .agent
            .post(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(issue)
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let created: GitLabIssue = response.body_mut().read_json()?;
        Ok(created)
    }

    /// Update an existing issue (GitLab uses PUT, not PATCH)
    pub fn update_issue(&self, iid: u64, update: &UpdateGitLabIssue) -> Result<GitLabIssue> {
        let url = self.project_url(&format!("/issues/{}", iid))?;

        let response = self
            .agent
            .put(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(update)
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let updated: GitLabIssue = response.body_mut().read_json()?;
        Ok(updated)
    }

    /// Delete an issue
    pub fn delete_issue(&self, iid: u64) -> Result<()> {
        let url = self.project_url(&format!("/issues/{}", iid))?;

        let response = self
            .agent
            .delete(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .call()
            .map_err(|e| self.handle_error(e))?;

        self.check_response(response)?;
        Ok(())
    }

    // ==================== Project Operations ====================

    /// List projects the authenticated user is a member of
    pub fn list_projects(&self) -> Result<Vec<GitLabProject>> {
        let url = format!(
            "{}/projects?membership=true&per_page=100&order_by=updated_at",
            self.base_url
        );

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let projects: Vec<GitLabProject> = response.body_mut().read_json()?;
        Ok(projects)
    }

    /// Get a project by ID or URL-encoded path
    pub fn get_project(&self, id: &str) -> Result<GitLabProject> {
        let url = format!("{}/projects/{}", self.base_url, urlencoding::encode(id));

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let project: GitLabProject = response.body_mut().read_json()?;
        Ok(project)
    }

    // ==================== Label Operations ====================

    /// List labels for the project
    pub fn list_labels(&self) -> Result<Vec<GitLabLabel>> {
        let url = self.project_url("/labels?per_page=100")?;

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let labels: Vec<GitLabLabel> = response.body_mut().read_json()?;
        Ok(labels)
    }

    /// Create a new label
    pub fn create_label(&self, label: &CreateGitLabLabel) -> Result<GitLabLabel> {
        let url = self.project_url("/labels")?;
        let response = self
            .agent
            .post(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(label)
            .map_err(|e| self.handle_error(e))?;
        let mut response = self.check_response(response)?;
        let created: GitLabLabel = response.body_mut().read_json()?;
        Ok(created)
    }

    /// Delete a label by ID
    pub fn delete_label(&self, label_id: u64) -> Result<()> {
        let url = self.project_url(&format!("/labels/{}", label_id))?;
        let response = self
            .agent
            .delete(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .call()
            .map_err(|e| self.handle_error(e))?;
        self.check_response(response)?;
        Ok(())
    }

    /// Update a label by ID
    pub fn update_label(&self, label_id: u64, update: &UpdateGitLabLabel) -> Result<GitLabLabel> {
        let url = self.project_url(&format!("/labels/{}", label_id))?;
        let response = self
            .agent
            .put(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(update)
            .map_err(|e| self.handle_error(e))?;
        let mut response = self.check_response(response)?;
        let label: GitLabLabel = response.body_mut().read_json()?;
        Ok(label)
    }

    // ==================== Note (Comment) Operations ====================

    /// Add a note to an issue
    pub fn add_note(&self, iid: u64, body: &str) -> Result<GitLabNote> {
        let url = self.project_url(&format!("/issues/{}/notes", iid))?;
        let note = CreateGitLabNote {
            body: body.to_string(),
        };

        let response = self
            .agent
            .post(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&note)
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let created: GitLabNote = response.body_mut().read_json()?;
        Ok(created)
    }

    /// Get notes on an issue, filtering out system-generated notes
    pub fn get_notes(&self, iid: u64) -> Result<Vec<GitLabNote>> {
        Ok(self
            .get_notes_page_raw(iid, 100, 1)?
            .into_iter()
            .filter(|n| !n.system)
            .collect())
    }

    /// Get a native GitLab notes page on an issue.
    pub(crate) fn get_notes_page_raw(
        &self,
        iid: u64,
        per_page: usize,
        page: usize,
    ) -> Result<Vec<GitLabNote>> {
        let url = self.project_url(&format!(
            "/issues/{}/notes?per_page={}&page={}",
            iid,
            per_page.min(100),
            page.max(1)
        ))?;

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let notes: Vec<GitLabNote> = response.body_mut().read_json()?;
        Ok(notes)
    }

    // ==================== Link Operations ====================

    /// Get issue links for an issue
    pub fn get_issue_links(&self, iid: u64) -> Result<Vec<GitLabIssueLink>> {
        let url = self.project_url(&format!("/issues/{}/links", iid))?;

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let links: Vec<GitLabIssueLink> = response.body_mut().read_json()?;
        Ok(links)
    }

    /// Create an issue link
    pub fn create_issue_link(&self, iid: u64, link: &CreateGitLabIssueLink) -> Result<()> {
        let url = self.project_url(&format!("/issues/{}/links", iid))?;

        let response = self
            .agent
            .post(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(link)
            .map_err(|e| self.handle_error(e))?;

        self.check_response(response)?;
        Ok(())
    }

    /// Delete an issue link by issue IID and link ID
    pub fn delete_issue_link(&self, iid: u64, issue_link_id: u64) -> Result<()> {
        let url = self.project_url(&format!("/issues/{}/links/{}", iid, issue_link_id))?;

        let response = self
            .agent
            .delete(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .call()
            .map_err(|e| self.handle_error(e))?;

        self.check_response(response)?;
        Ok(())
    }

    // ==================== Count Operations ====================

    /// Count issues matching search criteria by reading the X-Total header.
    /// Makes a minimal request with per_page=1 to get the count without
    /// transferring significant data.
    pub fn count_issues_by_query(&self, search: &str, state: Option<&str>) -> Result<Option<u64>> {
        let mut url = format!("{}?per_page=1&page=1", self.project_url("/issues")?);
        if !search.is_empty() {
            url.push_str(&format!("&search={}", urlencoding::encode(search)));
        }
        if let Some(s) = state {
            url.push_str(&format!("&state={}", urlencoding::encode(s)));
        }

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        // Read X-Total header before check_response takes ownership
        let total = response
            .headers()
            .get("x-total")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        // Validate the response status
        let _response = self.check_response(response)?;

        Ok(total)
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
        let graphql_url = self.graphql_url();

        let query = format!(
            r#"mutation {{ workItemUpdate(input: {{ id: "gid://gitlab/Issue/{}", hierarchyWidget: {{ parentId: "gid://gitlab/Issue/{}" }} }}) {{ workItem {{ id }} errors }} }}"#,
            child_global_id, parent_global_id
        );

        let body = serde_json::json!({ "query": query });

        let response = self
            .agent
            .post(&graphql_url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&body)
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let result: serde_json::Value = response.body_mut().read_json()?;

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
            return Err(GitLabError::Api {
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
            return Err(GitLabError::Api {
                status: 0,
                message: format!("Failed to set parent: {}", msg),
            });
        }

        Ok(())
    }

    // ==================== Member Operations ====================

    /// List project members (including inherited members)
    pub fn list_project_members(&self) -> Result<Vec<GitLabUser>> {
        let url = self.project_url("/members/all?per_page=100")?;

        let response = self
            .agent
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let members: Vec<GitLabUser> = response.body_mut().read_json()?;
        Ok(members)
    }
}
