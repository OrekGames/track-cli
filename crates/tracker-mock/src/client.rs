//! Mock client implementing IssueTracker trait
//!
//! Reads responses from fixture files instead of making HTTP requests.

use crate::manifest::{Manifest, ManifestError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracker_core::{
    Article, ArticleAttachment, Comment, CreateArticle, CreateIssue, CreateProject, Issue,
    IssueLink, IssueLinkType, IssueTag, IssueTracker, KnowledgeBase, Project, ProjectCustomField,
    Result, TrackerError, UpdateArticle, UpdateIssue, User,
};

/// A mock client that reads responses from fixture files
pub struct MockClient {
    /// Root directory containing the scenario
    scenario_dir: PathBuf,

    /// The loaded manifest
    manifest: Manifest,

    /// Track call counts for sequence responses
    call_counts: Mutex<HashMap<String, usize>>,

    /// Call log file writer
    log_writer: Mutex<Option<BufWriter<File>>>,

    /// Whether to log calls
    log_enabled: bool,
}

/// A single call log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallLogEntry {
    /// Timestamp of the call
    pub timestamp: DateTime<Utc>,

    /// Method name
    pub method: String,

    /// Arguments passed
    pub args: HashMap<String, serde_json::Value>,

    /// Response file used (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_file: Option<String>,

    /// Error message (if call failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Response status code
    pub status: u16,

    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl MockClient {
    /// Create a new MockClient from a scenario directory
    pub fn new(scenario_dir: impl AsRef<Path>) -> Result<Self> {
        let scenario_dir = scenario_dir.as_ref().to_path_buf();

        // Load manifest
        let manifest_path = scenario_dir.join("manifest.toml");
        let manifest = Manifest::load(&manifest_path)
            .map_err(|e| TrackerError::Io(format!("Failed to load mock manifest: {}", e)))?;

        // Open call log file
        let log_path = scenario_dir.join("call_log.jsonl");
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();

        let log_writer = log_file.map(BufWriter::new);

        Ok(Self {
            scenario_dir,
            manifest,
            call_counts: Mutex::new(HashMap::new()),
            log_writer: Mutex::new(log_writer),
            log_enabled: true,
        })
    }

    /// Create a MockClient without logging (for testing)
    pub fn new_without_logging(scenario_dir: impl AsRef<Path>) -> Result<Self> {
        let mut client = Self::new(scenario_dir)?;
        client.log_enabled = false;
        Ok(client)
    }

    /// Get the path to a response file
    fn response_path(&self, filename: &str) -> PathBuf {
        self.scenario_dir.join("responses").join(filename)
    }

    /// Load a response from a JSON file
    fn load_response<T: for<'de> Deserialize<'de>>(&self, filename: &str) -> Result<T> {
        let path = self.response_path(filename);
        let content = std::fs::read_to_string(&path).map_err(|e| {
            TrackerError::Io(format!(
                "Failed to read mock response {}: {}",
                path.display(),
                e
            ))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            TrackerError::Parse(format!("Failed to parse mock response {}: {}", filename, e))
        })
    }

    /// Find and load a response for a given method call
    fn get_response<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        args: HashMap<String, String>,
        body: Option<&str>,
    ) -> Result<T> {
        let start = std::time::Instant::now();

        // Get call counts
        let mut counts = self.call_counts.lock().unwrap();
        let request_key = Manifest::request_key(method, &args);

        // Find matching response
        let result = self
            .manifest
            .find_response(method, &args, body, &counts)
            .ok_or_else(|| ManifestError::NoMatch {
                method: method.to_string(),
                args: args.clone(),
            })
            .map_err(|e| TrackerError::Api {
                status: 404,
                message: e.to_string(),
            });

        // Increment call count
        *counts.entry(request_key).or_insert(0) += 1;
        drop(counts);

        let (mapping, filename) = result?;

        // Simulate delay if configured
        if mapping.delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(mapping.delay_ms));
        }

        // Check for error status
        if mapping.status >= 400 {
            let error_msg = self
                .load_response::<serde_json::Value>(&filename)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
                .unwrap_or_else(|| format!("HTTP {}", mapping.status));

            self.log_call(method, &args, None, Some(&error_msg), mapping.status, start);

            return Err(TrackerError::Api {
                status: mapping.status,
                message: error_msg,
            });
        }

        // Load and parse response
        let response: T = self.load_response(&filename)?;

        self.log_call(method, &args, Some(&filename), None, mapping.status, start);

        Ok(response)
    }

    /// Log a call to the call log file
    fn log_call(
        &self,
        method: &str,
        args: &HashMap<String, String>,
        response_file: Option<&str>,
        error: Option<&str>,
        status: u16,
        start: std::time::Instant,
    ) {
        if !self.log_enabled {
            return;
        }

        let entry = CallLogEntry {
            timestamp: Utc::now(),
            method: method.to_string(),
            args: args
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
            response_file: response_file.map(String::from),
            error: error.map(String::from),
            status,
            duration_ms: start.elapsed().as_millis() as u64,
        };

        if let Ok(mut writer) = self.log_writer.lock() {
            if let Some(w) = writer.as_mut() {
                if let Ok(json) = serde_json::to_string(&entry) {
                    let _ = writeln!(w, "{}", json);
                    let _ = w.flush();
                }
            }
        }
    }

    /// Get the call log entries
    pub fn read_call_log(&self) -> Result<Vec<CallLogEntry>> {
        let log_path = self.scenario_dir.join("call_log.jsonl");
        let content = std::fs::read_to_string(&log_path).unwrap_or_default();

        let entries: Vec<CallLogEntry> = content
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        Ok(entries)
    }

    /// Clear the call log
    pub fn clear_call_log(&self) -> Result<()> {
        let log_path = self.scenario_dir.join("call_log.jsonl");
        std::fs::write(&log_path, "")
            .map_err(|e| TrackerError::Io(format!("Failed to clear call log: {}", e)))
    }

    /// Get total number of calls made
    pub fn call_count(&self) -> usize {
        self.call_counts.lock().unwrap().values().sum()
    }
}

impl IssueTracker for MockClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        let args = [("id".to_string(), id.to_string())].into_iter().collect();
        self.get_response("get_issue", args, None)
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Issue>> {
        let args = [
            ("query".to_string(), query.to_string()),
            ("limit".to_string(), limit.to_string()),
            ("skip".to_string(), skip.to_string()),
        ]
        .into_iter()
        .collect();
        self.get_response("search_issues", args, None)
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        let args = [
            ("project".to_string(), issue.project_id.clone()),
            ("summary".to_string(), issue.summary.clone()),
        ]
        .into_iter()
        .collect();
        // Note: CreateIssue doesn't implement Serialize, so we just pass None for body
        self.get_response("create_issue", args, None)
    }

    fn update_issue(&self, id: &str, _update: &UpdateIssue) -> Result<Issue> {
        let args = [("id".to_string(), id.to_string())].into_iter().collect();
        // Note: UpdateIssue doesn't implement Serialize, so we just pass None for body
        self.get_response("update_issue", args, None)
    }

    fn delete_issue(&self, id: &str) -> Result<()> {
        let args = [("id".to_string(), id.to_string())].into_iter().collect();
        self.get_response("delete_issue", args, None)
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        self.get_response("list_projects", HashMap::new(), None)
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        let args = [("id".to_string(), id.to_string())].into_iter().collect();
        self.get_response("get_project", args, None)
    }

    fn create_project(&self, project: &CreateProject) -> Result<Project> {
        let args = [
            ("name".to_string(), project.name.clone()),
            ("short_name".to_string(), project.short_name.clone()),
        ]
        .into_iter()
        .collect();
        self.get_response("create_project", args, None)
    }

    fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        // Try to load from a specific response, or fall back to listing projects
        let args = [("identifier".to_string(), identifier.to_string())]
            .into_iter()
            .collect();

        if let Ok(id) = self.get_response::<String>("resolve_project_id", args, None) {
            return Ok(id);
        }

        // Fall back to listing projects and finding match
        let projects = self.list_projects()?;
        projects
            .iter()
            .find(|p| {
                p.short_name.eq_ignore_ascii_case(identifier)
                    || p.id == identifier
                    || p.name.eq_ignore_ascii_case(identifier)
            })
            .map(|p| p.id.clone())
            .ok_or_else(|| TrackerError::ProjectNotFound(identifier.to_string()))
    }

    fn get_project_custom_fields(&self, project_id: &str) -> Result<Vec<ProjectCustomField>> {
        let args = [("project_id".to_string(), project_id.to_string())]
            .into_iter()
            .collect();
        self.get_response("get_project_custom_fields", args, None)
    }

    fn list_project_users(&self, project_id: &str) -> Result<Vec<User>> {
        let args = [("project_id".to_string(), project_id.to_string())]
            .into_iter()
            .collect();
        self.get_response("list_project_users", args, None)
            .or_else(|_| Ok(vec![]))
    }

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        self.get_response("list_tags", HashMap::new(), None)
    }

    fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        self.get_response("list_link_types", HashMap::new(), None)
            .or_else(|_| Ok(vec![]))
    }

    fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        let args = [("issue_id".to_string(), issue_id.to_string())]
            .into_iter()
            .collect();
        self.get_response("get_issue_links", args, None)
    }

    fn link_issues(
        &self,
        source: &str,
        target: &str,
        link_type: &str,
        direction: &str,
    ) -> Result<()> {
        let args = [
            ("source".to_string(), source.to_string()),
            ("target".to_string(), target.to_string()),
            ("link_type".to_string(), link_type.to_string()),
            ("direction".to_string(), direction.to_string()),
        ]
        .into_iter()
        .collect();
        self.get_response("link_issues", args, None)
    }

    fn link_subtask(&self, child: &str, parent: &str) -> Result<()> {
        let args = [
            ("child".to_string(), child.to_string()),
            ("parent".to_string(), parent.to_string()),
        ]
        .into_iter()
        .collect();
        self.get_response("link_subtask", args, None)
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        let args = [
            ("issue_id".to_string(), issue_id.to_string()),
            ("text".to_string(), text.to_string()),
        ]
        .into_iter()
        .collect();
        self.get_response("add_comment", args, Some(text))
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        let args = [("issue_id".to_string(), issue_id.to_string())]
            .into_iter()
            .collect();
        self.get_response("get_comments", args, None)
    }
}

impl KnowledgeBase for MockClient {
    fn get_article(&self, id: &str) -> Result<Article> {
        let args = [("id".to_string(), id.to_string())].into_iter().collect();
        self.get_response("get_article", args, None)
    }

    fn list_articles(
        &self,
        project_id: Option<&str>,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<Article>> {
        let mut args: HashMap<String, String> = [
            ("limit".to_string(), limit.to_string()),
            ("skip".to_string(), skip.to_string()),
        ]
        .into_iter()
        .collect();

        if let Some(pid) = project_id {
            args.insert("project_id".to_string(), pid.to_string());
        }

        self.get_response("list_articles", args, None)
    }

    fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>> {
        let args = [
            ("query".to_string(), query.to_string()),
            ("limit".to_string(), limit.to_string()),
            ("skip".to_string(), skip.to_string()),
        ]
        .into_iter()
        .collect();
        self.get_response("search_articles", args, None)
    }

    fn create_article(&self, article: &CreateArticle) -> Result<Article> {
        let args = [
            ("project".to_string(), article.project_id.clone()),
            ("summary".to_string(), article.summary.clone()),
        ]
        .into_iter()
        .collect();
        self.get_response("create_article", args, None)
    }

    fn update_article(&self, id: &str, _update: &UpdateArticle) -> Result<Article> {
        let args = [("id".to_string(), id.to_string())].into_iter().collect();
        // Note: UpdateArticle doesn't implement Serialize, so we just pass None for body
        self.get_response("update_article", args, None)
    }

    fn delete_article(&self, id: &str) -> Result<()> {
        let args = [("id".to_string(), id.to_string())].into_iter().collect();
        self.get_response("delete_article", args, None)
    }

    fn get_child_articles(&self, parent_id: &str) -> Result<Vec<Article>> {
        let args = [("parent_id".to_string(), parent_id.to_string())]
            .into_iter()
            .collect();
        self.get_response("get_child_articles", args, None)
    }

    fn move_article(&self, article_id: &str, new_parent_id: Option<&str>) -> Result<Article> {
        let mut args: HashMap<String, String> =
            [("article_id".to_string(), article_id.to_string())]
                .into_iter()
                .collect();

        if let Some(parent) = new_parent_id {
            args.insert("new_parent_id".to_string(), parent.to_string());
        }

        self.get_response("move_article", args, None)
    }

    fn list_article_attachments(&self, article_id: &str) -> Result<Vec<ArticleAttachment>> {
        let args = [("article_id".to_string(), article_id.to_string())]
            .into_iter()
            .collect();
        self.get_response("list_article_attachments", args, None)
            .or_else(|_| Ok(vec![]))
    }

    fn get_article_comments(&self, article_id: &str) -> Result<Vec<Comment>> {
        let args = [("article_id".to_string(), article_id.to_string())]
            .into_iter()
            .collect();
        self.get_response("get_article_comments", args, None)
    }

    fn add_article_comment(&self, article_id: &str, text: &str) -> Result<Comment> {
        let args = [
            ("article_id".to_string(), article_id.to_string()),
            ("text".to_string(), text.to_string()),
        ]
        .into_iter()
        .collect();
        self.get_response("add_article_comment", args, Some(text))
    }
}

// MockClient is Send + Sync because all mutable state is behind Mutex
unsafe impl Send for MockClient {}
unsafe impl Sync for MockClient {}
