use crate::error::Result;
use crate::models::*;

/// Common trait for all issue tracker backends
///
/// This trait defines the operations that any issue tracker backend must implement.
/// Each backend (YouTrack, Jira, etc.) provides its own implementation.
pub trait IssueTracker: Send + Sync {
    // ========== Issue Operations ==========

    /// Get an issue by its ID
    fn get_issue(&self, id: &str) -> Result<Issue>;

    /// Search for issues using the backend's query language
    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Issue>>;

    /// Create a new issue
    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue>;

    /// Update an existing issue
    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue>;

    /// Delete an issue
    fn delete_issue(&self, id: &str) -> Result<()>;

    // ========== Project Operations ==========

    /// List all projects
    fn list_projects(&self) -> Result<Vec<Project>>;

    /// Get a project by ID
    fn get_project(&self, id: &str) -> Result<Project>;

    /// Resolve a project identifier (shortName or ID) to internal ID
    fn resolve_project_id(&self, identifier: &str) -> Result<String>;

    /// Get custom fields defined for a project
    fn get_project_custom_fields(&self, project_id: &str) -> Result<Vec<ProjectCustomField>>;

    // ========== Tag Operations ==========

    /// List all available tags
    fn list_tags(&self) -> Result<Vec<IssueTag>>;

    // ========== Link Operations ==========

    /// Get links for an issue
    fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>>;

    /// Link two issues together
    ///
    /// * `source` - Source issue ID
    /// * `target` - Target issue ID
    /// * `link_type` - Link type name (e.g., "Relates", "Depend", "Subtask")
    /// * `direction` - Link direction ("OUTWARD", "INWARD", "BOTH")
    fn link_issues(
        &self,
        source: &str,
        target: &str,
        link_type: &str,
        direction: &str,
    ) -> Result<()>;

    /// Create a subtask link (child is subtask of parent)
    fn link_subtask(&self, child: &str, parent: &str) -> Result<()>;

    // ========== Comment Operations ==========

    /// Add a comment to an issue
    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment>;

    /// Get comments for an issue
    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>>;
}
