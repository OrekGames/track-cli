//! Confluence REST API v2 client

use std::time::Duration;
use ureq::Agent;

use crate::error::{JiraError, Result};
use crate::models::confluence::*;

/// Confluence REST API client
///
/// Confluence uses the same authentication as Jira (Basic Auth with email + API token).
/// The API is available at `https://{domain}/wiki/api/v2` for Cloud instances.
pub struct ConfluenceClient {
    agent: Agent,
    base_url: String,
    auth_header: String,
}

impl ConfluenceClient {
    /// Create a new Confluence client
    ///
    /// The base_url should be the Confluence wiki URL (e.g., "https://example.atlassian.net/wiki")
    /// For Jira Cloud, Confluence is typically at the same domain with /wiki path.
    pub fn new(base_url: &str, email: &str, api_token: &str) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();

        // Base64 encode credentials for Basic Auth
        let credentials = format!("{}:{}", email, api_token);
        let encoded = base64_encode(&credentials);
        let auth_header = format!("Basic {}", encoded);

        // Ensure base_url ends with /wiki for proper API path construction
        let base_url = base_url.trim_end_matches('/');
        let base_url = if base_url.ends_with("/wiki") {
            base_url.to_string()
        } else {
            format!("{}/wiki", base_url)
        };

        Self {
            agent,
            base_url,
            auth_header,
        }
    }

    /// Get the v2 API URL
    fn api_v2_url(&self, path: &str) -> String {
        format!("{}/api/v2{}", self.base_url, path)
    }

    /// Get the v1 API URL (for search with CQL)
    fn api_v1_url(&self, path: &str) -> String {
        format!("{}/rest/api{}", self.base_url, path)
    }

    /// Check response status and return error if not successful
    fn check_response(
        &self,
        mut response: ureq::http::Response<ureq::Body>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let status = response.status().as_u16();

        if status >= 200 && status < 300 {
            return Ok(response);
        }

        // Try to read error body for better error messages
        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| String::new());

        // Try to parse as Confluence error response
        let message = if let Ok(error_response) = serde_json::from_str::<serde_json::Value>(&body) {
            let mut messages = Vec::new();

            // Confluence error format: {"message": "...", "statusCode": 404}
            if let Some(msg) = error_response.get("message").and_then(|m| m.as_str()) {
                messages.push(msg.to_string());
            }

            // Also check for errors array
            if let Some(errors) = error_response.get("errors").and_then(|e| e.as_array()) {
                for e in errors {
                    if let Some(msg) = e.get("message").and_then(|m| m.as_str()) {
                        messages.push(msg.to_string());
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
        } else if status == 404 {
            Err(JiraError::Api {
                status,
                message: "Not found".to_string(),
            })
        } else {
            Err(JiraError::Api { status, message })
        }
    }

    // ==================== Space Operations ====================

    /// List all accessible spaces
    pub fn list_spaces(&self, limit: usize) -> Result<ConfluenceSpaceList> {
        let url = format!("{}?limit={}", self.api_v2_url("/spaces"), limit);

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let spaces: ConfluenceSpaceList = response.body_mut().read_json()?;
        Ok(spaces)
    }

    /// Get a space by ID
    pub fn get_space(&self, space_id: &str) -> Result<ConfluenceSpace> {
        let url = self.api_v2_url(&format!("/spaces/{}", space_id));

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let space: ConfluenceSpace = response.body_mut().read_json()?;
        Ok(space)
    }

    // ==================== Page Operations ====================

    /// Get a page by ID
    pub fn get_page(&self, page_id: &str) -> Result<ConfluencePage> {
        let url = format!(
            "{}?body-format=storage",
            self.api_v2_url(&format!("/pages/{}", page_id))
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let page: ConfluencePage = response.body_mut().read_json()?;
        Ok(page)
    }

    /// List pages, optionally filtered by space
    pub fn list_pages(
        &self,
        space_id: Option<&str>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<ConfluencePageList> {
        let mut url = format!(
            "{}?limit={}&body-format=storage&status=current",
            self.api_v2_url("/pages"),
            limit
        );

        if let Some(sid) = space_id {
            url.push_str(&format!("&space-id={}", sid));
        }

        if let Some(c) = cursor {
            url.push_str(&format!("&cursor={}", urlencoding::encode(c)));
        }

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let pages: ConfluencePageList = response.body_mut().read_json()?;
        Ok(pages)
    }

    /// Search pages using CQL (v1 API)
    pub fn search_pages(&self, query: &str, limit: usize, start: usize) -> Result<ConfluenceSearchResult> {
        // Use CQL to search for pages
        let cql = format!("type=page AND text~\"{}\"", query.replace('"', "\\\""));
        let cql_encoded = urlencoding::encode(&cql);

        let url = format!(
            "{}?cql={}&limit={}&start={}&expand=content.space",
            self.api_v1_url("/search"),
            cql_encoded,
            limit,
            start
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let result: ConfluenceSearchResult = response.body_mut().read_json()?;
        Ok(result)
    }

    /// Create a new page
    pub fn create_page(&self, page: &CreateConfluencePage) -> Result<ConfluencePage> {
        let url = self.api_v2_url("/pages");

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(page)
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let created: ConfluencePage = response.body_mut().read_json()?;
        Ok(created)
    }

    /// Update an existing page
    pub fn update_page(&self, page_id: &str, update: &UpdateConfluencePage) -> Result<ConfluencePage> {
        let url = self.api_v2_url(&format!("/pages/{}", page_id));

        let response = self
            .agent
            .put(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(update)
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let updated: ConfluencePage = response.body_mut().read_json()?;
        Ok(updated)
    }

    /// Delete a page
    pub fn delete_page(&self, page_id: &str) -> Result<()> {
        let url = self.api_v2_url(&format!("/pages/{}", page_id));

        let response = self
            .agent
            .delete(&url)
            .header("Authorization", &self.auth_header)
            .call()
            .map_err(JiraError::from)?;

        self.check_response(response)?;
        Ok(())
    }

    /// Get child pages of a parent page
    pub fn get_child_pages(&self, parent_id: &str, limit: usize) -> Result<ConfluenceChildrenResponse> {
        let url = format!(
            "{}?limit={}",
            self.api_v2_url(&format!("/pages/{}/children", parent_id)),
            limit
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let children: ConfluenceChildrenResponse = response.body_mut().read_json()?;
        Ok(children)
    }

    // ==================== Comment Operations ====================

    /// Get footer comments on a page
    pub fn get_page_comments(&self, page_id: &str, limit: usize) -> Result<ConfluenceCommentList> {
        let url = format!(
            "{}?limit={}&body-format=storage",
            self.api_v2_url(&format!("/pages/{}/footer-comments", page_id)),
            limit
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let comments: ConfluenceCommentList = response.body_mut().read_json()?;
        Ok(comments)
    }

    /// Add a footer comment to a page
    pub fn add_page_comment(&self, page_id: &str, body: &str) -> Result<ConfluenceComment> {
        let url = self.api_v2_url("/footer-comments");

        let comment = CreateConfluenceComment {
            page_id: page_id.to_string(),
            body: CreateConfluenceBody {
                representation: "storage".to_string(),
                value: format!("<p>{}</p>", html_escape(body)),
            },
        };

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&comment)
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let created: ConfluenceComment = response.body_mut().read_json()?;
        Ok(created)
    }

    // ==================== Attachment Operations ====================

    /// List attachments on a page
    pub fn get_page_attachments(&self, page_id: &str, limit: usize) -> Result<ConfluenceAttachmentList> {
        let url = format!(
            "{}?limit={}",
            self.api_v2_url(&format!("/pages/{}/attachments", page_id)),
            limit
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header)
            .header("Accept", "application/json")
            .call()
            .map_err(JiraError::from)?;

        let mut response = self.check_response(response)?;
        let attachments: ConfluenceAttachmentList = response.body_mut().read_json()?;
        Ok(attachments)
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

/// Simple HTML escape for comment body
fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
