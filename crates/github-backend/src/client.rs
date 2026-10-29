use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::OnceLock;
use tracker_core::http::{self, Response, read_json};
use tracker_core::{Result, TrackerError};
use ureq::typestate::{WithBody, WithoutBody};
use ureq::{Agent, RequestBuilder};

use crate::convert::convert_query_to_github;
use crate::models::*;
use crate::wiki::WikiManager;

/// Media type GitHub recommends for REST API requests.
const GITHUB_ACCEPT: &str = "application/vnd.github+json";
/// REST API version the request and response models are written against.
const GITHUB_API_VERSION: &str = "2022-11-28";

/// Message of the error returned when GitHub's rate limit is exhausted.
const RATE_LIMIT_MESSAGE: &str = "GitHub API rate limit exceeded";

/// The error for a 403 with no rate-limit quota left. It reads as an HTTP 429
/// so doctor and the CLI treat it as rate limiting, not a permissions problem.
fn rate_limit_error() -> TrackerError {
    TrackerError::Api {
        status: 429,
        message: RATE_LIMIT_MESSAGE.to_string(),
    }
}

/// Whether `err` is GitHub's exhausted rate limit (a 403 with
/// `x-ratelimit-remaining: 0`), as opposed to any other 403 or a plain HTTP 429.
pub fn is_rate_limit_error(err: &TrackerError) -> bool {
    matches!(err, TrackerError::Api { status: 429, message } if message == RATE_LIMIT_MESSAGE)
}

/// Derive the web host (which serves both the UI and git) from the REST API
/// base URL.
///
/// `https://api.github.com` (and GHE.com's `https://api.<tenant>.ghe.com`) drop
/// the `api.` subdomain, while GitHub Enterprise Server serves its API under
/// `https://<host>/api/v3`. Any other URL is assumed to already be the web host.
pub(crate) fn web_base_url(api_base_url: &str) -> String {
    let api_base_url = api_base_url.trim_end_matches('/');
    if let Some(host_root) = api_base_url.strip_suffix("/api/v3") {
        return host_root.to_string();
    }
    match api_base_url.split_once("://api.") {
        Some((scheme, host)) => format!("{}://{}", scheme, host),
        None => api_base_url.to_string(),
    }
}

/// GitHub REST API client
pub struct GitHubClient {
    agent: Agent,
    base_url: String,
    owner: String,
    repo: String,
    token: String,
    wiki_manager: OnceLock<WikiManager>,
}

impl GitHubClient {
    /// Create a new GitHub client targeting api.github.com
    pub fn new(owner: &str, repo: &str, token: &str) -> Self {
        Self::with_base_url("https://api.github.com", owner, repo, token)
    }

    /// Get wiki manager, lazily initializing on first access
    pub fn wiki(&self) -> &WikiManager {
        self.wiki_manager
            .get_or_init(|| WikiManager::new(&self.base_url, &self.owner, &self.repo, &self.token))
    }

    /// Create a new GitHub client with a custom base URL (for GitHub Enterprise or testing)
    pub fn with_base_url(base_url: &str, owner: &str, repo: &str, token: &str) -> Self {
        Self {
            agent: http::agent(),
            base_url: base_url.trim_end_matches('/').to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            token: token.to_string(),
            wiki_manager: OnceLock::new(),
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

    /// Browser URL of the repository's issue list, or of `number` within
    /// `repo_path` (`owner/repo`), defaulting to the configured repository.
    pub(crate) fn issues_web_url(&self, repo_path: Option<&str>, number: Option<u64>) -> String {
        let configured_repo_path = format!("{}/{}", self.owner, self.repo);
        let issues_url = format!(
            "{}/{}/issues",
            web_base_url(&self.base_url),
            repo_path.unwrap_or(&configured_repo_path)
        );
        match number {
            // GitHub redirects /issues/N to /pull/N when N is a pull request.
            Some(number) => format!("{issues_url}/{number}"),
            None => issues_url,
        }
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

    /// Add the authentication and API-version headers every request carries.
    fn with_github_headers<B>(&self, request: RequestBuilder<B>) -> RequestBuilder<B> {
        request
            .header("Authorization", &self.auth_header())
            .header("Accept", GITHUB_ACCEPT)
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
    }

    /// Send a body-less request (GET, DELETE) and return the successful response.
    fn call(&self, request: RequestBuilder<WithoutBody>) -> Result<Response> {
        let response = self.with_github_headers(request).call()?;
        self.check_response(response)
    }

    /// Send `body` as JSON and return the successful response.
    fn send_json(
        &self,
        request: RequestBuilder<WithBody>,
        body: &impl Serialize,
    ) -> Result<Response> {
        let response = self
            .with_github_headers(request)
            .header("Content-Type", "application/json")
            .send_json(body)?;
        self.check_response(response)
    }

    /// GET `url` and decode the JSON response.
    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        read_json(self.call(self.agent.get(url))?)
    }

    /// Check response status and return error if not successful
    fn check_response(&self, response: Response) -> Result<Response> {
        // Detect rate limiting: 403 with x-ratelimit-remaining: 0
        if response.status().as_u16() == 403
            && let Some(remaining) = response.headers().get("x-ratelimit-remaining")
            && remaining.to_str().unwrap_or("") == "0"
        {
            return Err(rate_limit_error());
        }

        http::check_response(response, error_message)
    }

    // ==================== Issue Operations ====================

    /// Get an issue by number
    pub fn get_issue(&self, number: u64) -> Result<GitHubIssue> {
        self.get_json(&self.repo_url(&format!("/issues/{}", number)))
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

        self.get_json(&url)
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
        let request = self.agent.post(&self.repo_url("/issues"));
        read_json(self.send_json(request, issue)?)
    }

    /// Update an existing issue
    pub fn update_issue(&self, number: u64, update: &UpdateGitHubIssue) -> Result<GitHubIssue> {
        let request = self
            .agent
            .patch(&self.repo_url(&format!("/issues/{}", number)));
        read_json(self.send_json(request, update)?)
    }

    // ==================== Sub-Issue Operations ====================

    /// Add an issue as a sub-issue of a parent issue.
    ///
    /// `parent_number` is the issue number of the parent.
    /// `child_id` is the **global numeric ID** (not the issue number) of the child.
    pub fn add_sub_issue(&self, parent_number: u64, child_id: u64) -> Result<()> {
        let request = self
            .agent
            .post(&self.repo_url(&format!("/issues/{}/sub_issues", parent_number)));
        let body = serde_json::json!({
            "sub_issue_id": child_id,
            "replace_parent": true
        });

        self.send_json(request, &body)?;
        Ok(())
    }

    // ==================== Repository Operations ====================

    /// List repositories for the authenticated user
    pub fn list_repos(&self) -> Result<Vec<GitHubRepo>> {
        self.get_json(&format!(
            "{}/user/repos?per_page=100&sort=updated",
            self.base_url
        ))
    }

    /// Get a specific repository
    pub fn get_repo(&self, owner: &str, repo: &str) -> Result<GitHubRepo> {
        self.get_json(&format!("{}/repos/{}/{}", self.base_url, owner, repo))
    }

    // ==================== Label Operations ====================

    /// List every label of the repository
    pub fn list_labels(&self) -> Result<Vec<GitHubLabel>> {
        self.get_all_pages(&self.repo_url("/labels"))
    }

    /// Create a label
    pub fn create_label(&self, label: &CreateGitHubLabel) -> Result<GitHubLabel> {
        let request = self.agent.post(&self.repo_url("/labels"));
        read_json(self.send_json(request, label)?)
    }

    /// Delete a label by name
    pub fn delete_label(&self, name: &str) -> Result<()> {
        let url = self.repo_url(&format!("/labels/{}", urlencoding::encode(name)));
        self.call(self.agent.delete(&url))?;
        Ok(())
    }

    /// Update a label by name
    pub fn update_label(&self, name: &str, update: &UpdateGitHubLabel) -> Result<GitHubLabel> {
        let url = self.repo_url(&format!("/labels/{}", urlencoding::encode(name)));
        read_json(self.send_json(self.agent.patch(&url), update)?)
    }

    // ==================== Comment Operations ====================

    /// Add a comment to an issue
    pub fn add_comment(&self, number: u64, body: &str) -> Result<GitHubComment> {
        let request = self
            .agent
            .post(&self.repo_url(&format!("/issues/{}/comments", number)));
        let comment = CreateGitHubComment {
            body: body.to_string(),
        };

        read_json(self.send_json(request, &comment)?)
    }

    /// Get comments on an issue
    pub fn get_comments(&self, number: u64) -> Result<Vec<GitHubComment>> {
        self.get_comments_page(number, 100, 1)
    }

    /// Get a native GitHub comment page for an issue
    pub fn get_comments_page(
        &self,
        number: u64,
        per_page: usize,
        page: usize,
    ) -> Result<Vec<GitHubComment>> {
        self.get_json(&format!(
            "{}?per_page={}&page={}",
            self.repo_url(&format!("/issues/{}/comments", number)),
            per_page.min(100),
            page.max(1)
        ))
    }

    // ==================== Timeline / History Operations ====================

    /// Fetch an issue's complete timeline, paging through every event.
    ///
    /// Events are returned oldest-first, matching the GitHub API ordering.
    pub fn get_issue_timeline(&self, number: u64) -> Result<Vec<GitHubTimelineEvent>> {
        let raw: Vec<serde_json::Value> =
            self.get_all_pages(&self.repo_url(&format!("/issues/{}/timeline", number)))?;
        Ok(raw
            .into_iter()
            .map(GitHubTimelineEvent::from_value)
            .collect::<serde_json::Result<_>>()?)
    }

    /// GET every page of a list endpoint (`url` without a query string), in API order.
    ///
    /// GitHub honors `per_page`, so a short page is a reliable end-of-data
    /// signal. A `MAX_PAGES` backstop guards against a server that ignores
    /// paging and never shrinks a page — failing loudly via
    /// [`TrackerError::PaginationStalled`] instead of looping forever.
    fn get_all_pages<T: DeserializeOwned>(&self, url: &str) -> Result<Vec<T>> {
        const PER_PAGE: usize = 100;
        const MAX_PAGES: usize = 10_000;

        let mut items = Vec::new();
        for page in 1..=MAX_PAGES {
            let batch: Vec<T> =
                self.get_json(&format!("{}?per_page={}&page={}", url, PER_PAGE, page))?;
            let fetched = batch.len();
            items.extend(batch);
            if fetched < PER_PAGE {
                return Ok(items);
            }
        }
        Err(TrackerError::PaginationStalled(format!(
            "'{}' did not terminate after {} pages",
            url, MAX_PAGES
        )))
    }
}

/// GitHub error bodies carry a `message`; `errors[]` and `documentation_url` are not shown.
fn error_message(json: &Value) -> Option<String> {
    json.get("message")?.as_str().map(str::to_string)
}
