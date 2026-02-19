use std::time::Duration;
use ureq::Agent;

use crate::convert::convert_query_to_github;
use crate::error::{GitHubError, Result};
use crate::models::*;
use crate::wiki::WikiManager;

/// GitHub REST API client
pub struct GitHubClient {
    agent: Agent,
    base_url: String,
    owner: String,
    repo: String,
    token: String,
    wiki_manager: WikiManager,
}

impl GitHubClient {
    /// Create a new GitHub client targeting api.github.com
    pub fn new(owner: &str, repo: &str, token: &str) -> Self {
        Self::with_base_url("https://api.github.com", owner, repo, token)
    }

    /// Get or initialize the wiki manager
    pub fn get_or_init_wiki(&mut self) -> Result<&mut WikiManager> {
        Ok(&mut self.wiki_manager)
    }

    /// Get wiki manager (read-only)
    pub fn wiki(&self) -> &WikiManager {
        &self.wiki_manager
    }

    /// Create a new GitHub client with a custom base URL (for GitHub Enterprise or testing)
    pub fn with_base_url(base_url: &str, owner: &str, repo: &str, token: &str) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();

        let wiki_manager = WikiManager::new(owner, repo, token)
            .unwrap_or_else(|_| panic!("Failed to initialize WikiManager"));

        Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            token: token.to_string(),
            wiki_manager,
        }
    }

    /// Get the owner for this client
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// Get the repo for this client
    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// Build a repo-scoped URL
    fn repo_url(&self, path: &str) -> String {
        format!(
            "{}/repos/{}/{}{}",
            self.base_url, self.owner, self.repo, path
        )
    }

    /// Build the Authorization header value
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
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

        // Detect rate limiting: 403 with x-ratelimit-remaining: 0
        if status == 403 {
            if let Some(remaining) = response.headers().get("x-ratelimit-remaining") {
                if remaining.to_str().unwrap_or("") == "0" {
                    return Err(GitHubError::RateLimited);
                }
            }
        }

        // Try to read error body
        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| String::new());

        // Try to parse as GitHub error response
        let message = if let Ok(error_response) = serde_json::from_str::<serde_json::Value>(&body) {
            error_response
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or(&body)
                .to_string()
        } else if body.is_empty() {
            format!("HTTP {}", status)
        } else {
            body
        };

        if status == 401 {
            Err(GitHubError::Unauthorized)
        } else {
            Err(GitHubError::Api { status, message })
        }
    }

    // ==================== Issue Operations ====================

    /// Get an issue by number
    pub fn get_issue(&self, number: u64) -> Result<GitHubIssue> {
        let url = self.repo_url(&format!("/issues/{}", number));

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let issue: GitHubIssue = response.body_mut().read_json()?;
        Ok(issue)
    }

    /// List issues for the repository
    ///
    /// Returns only actual issues, filtering out pull requests.
    pub fn list_issues(
        &self,
        state: &str,
        per_page: usize,
        page: usize,
    ) -> Result<Vec<GitHubIssue>> {
        let url = format!(
            "{}?state={}&per_page={}&page={}",
            self.repo_url("/issues"),
            urlencoding::encode(state),
            per_page,
            page
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let issues: Vec<GitHubIssue> = response.body_mut().read_json()?;

        // Filter out pull requests
        Ok(issues
            .into_iter()
            .filter(|i| !i.is_pull_request())
            .collect())
    }

    /// Search issues using GitHub search syntax
    ///
    /// The query is automatically scoped to this repository.
    pub fn search_issues(
        &self,
        query: &str,
        per_page: usize,
        page: usize,
    ) -> Result<GitHubSearchResult> {
        let scoped_query = format!("repo:{}/{} {}", self.owner, self.repo, query);
        let url = format!(
            "{}/search/issues?q={}&per_page={}&page={}",
            self.base_url,
            urlencoding::encode(&scoped_query),
            per_page,
            page
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let result: GitHubSearchResult = response.body_mut().read_json()?;
        Ok(result)
    }

    /// Count issues matching a query without fetching full issue data.
    /// Converts the query to GitHub format internally, then searches with per_page=1.
    pub fn count_issues(&self, query: &str) -> Result<u64> {
        let github_query = convert_query_to_github(query);
        let result = self.search_issues(&github_query, 1, 1)?;
        Ok(result.total_count)
    }

    /// Create a new issue
    pub fn create_issue(&self, issue: &CreateGitHubIssue) -> Result<GitHubIssue> {
        let url = self.repo_url("/issues");

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send_json(issue)
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let created: GitHubIssue = response.body_mut().read_json()?;
        Ok(created)
    }

    /// Update an existing issue
    pub fn update_issue(&self, number: u64, update: &UpdateGitHubIssue) -> Result<GitHubIssue> {
        let url = self.repo_url(&format!("/issues/{}", number));

        let response = self
            .agent
            .patch(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send_json(update)
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let updated: GitHubIssue = response.body_mut().read_json()?;
        Ok(updated)
    }

    // ==================== Repository Operations ====================

    /// List repositories for the authenticated user
    pub fn list_repos(&self) -> Result<Vec<GitHubRepo>> {
        let url = format!("{}/user/repos?per_page=100&sort=updated", self.base_url);

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let repos: Vec<GitHubRepo> = response.body_mut().read_json()?;
        Ok(repos)
    }

    /// Get a specific repository
    pub fn get_repo(&self, owner: &str, repo: &str) -> Result<GitHubRepo> {
        let url = format!("{}/repos/{}/{}", self.base_url, owner, repo);

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let repo: GitHubRepo = response.body_mut().read_json()?;
        Ok(repo)
    }

    // ==================== Label Operations ====================

    /// List labels for the repository
    pub fn list_labels(&self) -> Result<Vec<GitHubLabel>> {
        let url = format!("{}?per_page=100", self.repo_url("/labels"));

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let labels: Vec<GitHubLabel> = response.body_mut().read_json()?;
        Ok(labels)
    }

    /// Create a label
    pub fn create_label(&self, label: &CreateGitHubLabel) -> Result<GitHubLabel> {
        let url = self.repo_url("/labels");

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send_json(label)
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let created: GitHubLabel = response.body_mut().read_json()?;
        Ok(created)
    }

    /// Delete a label by name
    pub fn delete_label(&self, name: &str) -> Result<()> {
        let encoded_name = urlencoding::encode(name);
        let url = self.repo_url(&format!("/labels/{}", encoded_name));

        let response = self
            .agent
            .delete(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(GitHubError::Http)?;

        self.check_response(response)?;
        Ok(())
    }

    /// Update a label by name
    pub fn update_label(&self, name: &str, update: &UpdateGitHubLabel) -> Result<GitHubLabel> {
        let encoded_name = urlencoding::encode(name);
        let url = self.repo_url(&format!("/labels/{}", encoded_name));

        let response = self
            .agent
            .patch(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send_json(update)
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let label: GitHubLabel = response.body_mut().read_json()?;
        Ok(label)
    }

    // ==================== Comment Operations ====================

    /// Add a comment to an issue
    pub fn add_comment(&self, number: u64, body: &str) -> Result<GitHubComment> {
        let url = self.repo_url(&format!("/issues/{}/comments", number));

        let comment = CreateGitHubComment {
            body: body.to_string(),
        };

        let response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send_json(&comment)
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let created: GitHubComment = response.body_mut().read_json()?;
        Ok(created)
    }

    /// Get comments on an issue
    pub fn get_comments(&self, number: u64) -> Result<Vec<GitHubComment>> {
        let url = format!(
            "{}?per_page=100",
            self.repo_url(&format!("/issues/{}/comments", number))
        );

        let response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(GitHubError::Http)?;

        let mut response = self.check_response(response)?;
        let comments: Vec<GitHubComment> = response.body_mut().read_json()?;
        Ok(comments)
    }
}
