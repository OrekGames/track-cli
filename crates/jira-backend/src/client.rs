use std::time::Duration;
use ureq::Agent;

use crate::error::{JiraError, Result};
use crate::models::*;

/// Default fields to request for issues
const DEFAULT_ISSUE_FIELDS: &[&str] = &[
    "summary",
    "description",
    "status",
    "priority",
    "issuetype",
    "project",
    "assignee",
    "reporter",
    "labels",
    "created",
    "updated",
    "subtasks",
    "parent",
    "issuelinks",
];

/// Jira REST API client
pub struct JiraClient {
    agent: Agent,
    base_url: String,
    auth_header: String,
}

impl JiraClient {
    /// Create a new Jira client with Basic Auth
    ///
    /// For Jira Cloud, use your email and an API token.
    /// For Jira Server, use your username and password.
    pub fn new(base_url: &str, email: &str, api_token: &str) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            // Don't treat HTTP errors as errors - we'll handle them ourselves
            .http_status_as_error(false)
            .build()
            .into();

        // Base64 encode credentials for Basic Auth
        let credentials = format!("{}:{}", email, api_token);
        let encoded = base64_encode(&credentials);
        let auth_header = format!("Basic {}", encoded);

        Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_header,
        }
    }

    /// Get the API base URL (v3 for Cloud, v2 for Server)
    fn api_url(&self, path: &str) -> String {
        format!("{}/rest/api/3{}", self.base_url, path)
    }

    /// Handle HTTP errors
    fn handle_error(&self, err: ureq::Error) -> JiraError {
        match &err {
            ureq::Error::StatusCode(status) => {
                if *status == 401 {
                    JiraError::Unauthorized
                } else if *status == 404 {
                    JiraError::Api {
                        status: *status,
                        message: "Not found".to_string(),
                    }
                } else {
                    JiraError::Api {
                        status: *status,
                        message: format!("HTTP {}", status),
                    }
                }
            }
            _ => JiraError::Http(err),
        }
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
        let fields = DEFAULT_ISSUE_FIELDS.join(",");
        let url = format!(
            "{}?fields={}",
            self.api_url(&format!("/issue/{}", key)),
            fields
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let issue: JiraIssue = response.body_mut().read_json()?;
        Ok(issue)
    }

    /// Search issues using JQL
    pub fn search_issues(
        &self,
        jql: &str,
        max_results: usize,
        start_at: usize,
    ) -> Result<JiraSearchResult> {
        // Note: Jira Cloud now uses /search/jql with GET request (as of 2024)
        let fields = DEFAULT_ISSUE_FIELDS.join(",");
        let jql_encoded = urlencoding::encode(jql);
        let url = format!(
            "{}?jql={}&startAt={}&maxResults={}&fields={}",
            self.api_url("/search/jql"),
            jql_encoded,
            start_at,
            max_results,
            fields
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let result: JiraSearchResult = response.body_mut().read_json()?;
        Ok(result)
    }

    /// Create a new issue
    pub fn create_issue(&self, issue: &CreateJiraIssue) -> Result<JiraIssue> {
        let url = self.api_url("/issue");

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(issue)
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;

        // Jira returns a minimal response on create, we need to fetch the full issue
        #[derive(serde::Deserialize)]
        struct CreateResponse {
            #[allow(dead_code)]
            id: String,
            key: String,
        }
        let created: CreateResponse = response.body_mut().read_json()?;

        // Fetch the full issue
        self.get_issue(&created.key)
    }

    /// Update an existing issue
    pub fn update_issue(&self, key: &str, update: &UpdateJiraIssue) -> Result<JiraIssue> {
        let url = self.api_url(&format!("/issue/{}", key));

        let response = self
            .agent
            .put(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(update)
            .map_err(|e| self.handle_error(e))?;

        self.check_response(response)?;

        // Jira returns 204 No Content on update, fetch the updated issue
        self.get_issue(key)
    }

    /// Delete an issue
    pub fn delete_issue(&self, key: &str) -> Result<()> {
        let url = self.api_url(&format!("/issue/{}", key));

        let response = self
            .agent
            .delete(&url)
            .header("Authorization", &self.auth_header)
            .call()
            .map_err(|e| self.handle_error(e))?;

        self.check_response(response)?;
        Ok(())
    }

    // ==================== Project Operations ====================

    /// List all accessible projects
    pub fn list_projects(&self) -> Result<Vec<JiraProject>> {
        let url = self.api_url("/project");

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let projects: Vec<JiraProject> = response.body_mut().read_json()?;
        Ok(projects)
    }

    /// Get a project by key or ID
    pub fn get_project(&self, key: &str) -> Result<JiraProject> {
        let url = self.api_url(&format!("/project/{}", key));

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let project: JiraProject = response.body_mut().read_json()?;
        Ok(project)
    }

    // ==================== Comment Operations ====================

    /// Add a comment to an issue
    pub fn add_comment(&self, key: &str, body: &str) -> Result<JiraComment> {
        let url = self.api_url(&format!("/issue/{}/comment", key));

        let comment = CreateJiraComment::from_text(body);

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&comment)
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let created: JiraComment = response.body_mut().read_json()?;
        Ok(created)
    }

    /// Get comments on an issue
    pub fn get_comments(&self, key: &str) -> Result<Vec<JiraComment>> {
        let url = self.api_url(&format!("/issue/{}/comment", key));

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let comments: JiraCommentsResponse = response.body_mut().read_json()?;
        Ok(comments.comments)
    }

    // ==================== Link Operations ====================

    /// Create a link between two issues
    pub fn create_link(&self, link: &CreateJiraIssueLink) -> Result<()> {
        let url = self.api_url("/issueLink");

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .send_json(link)
            .map_err(|e| self.handle_error(e))?;

        self.check_response(response)?;
        Ok(())
    }

    // ==================== Label Operations ====================

    /// List all labels in the Jira instance
    pub fn list_labels(&self) -> Result<Vec<String>> {
        let url = self.api_url("/label");
        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;
        let mut response = self.check_response(response)?;

        // Jira returns: {"total": N, "maxResults": N, "values": ["label1", "label2"]}
        #[derive(serde::Deserialize)]
        struct LabelResponse {
            values: Vec<String>,
        }
        let result: LabelResponse = response.body_mut().read_json()?;
        Ok(result.values)
    }

    /// List all issue link types
    pub fn list_link_types(&self) -> Result<Vec<JiraIssueLinkType>> {
        let url = self.api_url("/issueLinkType");

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct LinkTypesResponse {
            issue_link_types: Vec<JiraIssueLinkType>,
        }

        let result: LinkTypesResponse = response.body_mut().read_json()?;
        Ok(result.issue_link_types)
    }

    /// List users assignable to issues in a project
    pub fn list_assignable_users(&self, project_key: &str) -> Result<Vec<JiraUser>> {
        let url = format!(
            "{}?project={}",
            self.api_url("/user/assignable/search"),
            urlencoding::encode(project_key)
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let mut response = self.check_response(response)?;
        let users: Vec<JiraUser> = response.body_mut().read_json()?;
        Ok(users)
    }
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
