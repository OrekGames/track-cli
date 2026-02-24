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
    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<SearchResult<Issue>>;

    /// Get the count of issues matching a query, without fetching the issues themselves.
    /// Returns None if the backend does not support count queries.
    /// Default implementation returns None (opt-in per backend).
    fn get_issue_count(&self, query: &str) -> Result<Option<u64>> {
        let _ = query;
        Ok(None)
    }

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

    /// Create a new project
    fn create_project(&self, project: &CreateProject) -> Result<Project>;

    /// Resolve a project identifier (shortName or ID) to internal ID
    fn resolve_project_id(&self, identifier: &str) -> Result<String>;

    /// Get custom fields defined for a project
    fn get_project_custom_fields(&self, project_id: &str) -> Result<Vec<ProjectCustomField>>;

    /// List users that can be assigned to issues in a project
    fn list_project_users(&self, project_id: &str) -> Result<Vec<User>> {
        // Default implementation returns empty list
        let _ = project_id;
        Ok(Vec::new())
    }

    // ========== Custom Field Admin Operations ==========

    /// List all custom field definitions (instance-wide)
    fn list_custom_field_definitions(&self) -> Result<Vec<CustomFieldDefinition>> {
        Err(crate::error::TrackerError::InvalidInput(
            "Custom field management not supported by this backend".to_string(),
        ))
    }

    /// Create a new custom field definition
    fn create_custom_field(&self, field: &CreateCustomField) -> Result<CustomFieldDefinition> {
        let _ = field;
        Err(crate::error::TrackerError::InvalidInput(
            "Custom field creation not supported by this backend".to_string(),
        ))
    }

    /// List all bundles of a given type
    fn list_bundles(&self, bundle_type: BundleType) -> Result<Vec<BundleDefinition>> {
        let _ = bundle_type;
        Err(crate::error::TrackerError::InvalidInput(
            "Bundle management not supported by this backend".to_string(),
        ))
    }

    /// Create a new bundle with values
    fn create_bundle(&self, bundle: &CreateBundle) -> Result<BundleDefinition> {
        let _ = bundle;
        Err(crate::error::TrackerError::InvalidInput(
            "Bundle creation not supported by this backend".to_string(),
        ))
    }

    /// Add values to an existing bundle
    fn add_bundle_values(
        &self,
        bundle_id: &str,
        bundle_type: BundleType,
        values: &[CreateBundleValue],
    ) -> Result<Vec<BundleValueDefinition>> {
        let _ = (bundle_id, bundle_type, values);
        Err(crate::error::TrackerError::InvalidInput(
            "Bundle modification not supported by this backend".to_string(),
        ))
    }

    /// Attach a custom field to a project
    fn attach_field_to_project(
        &self,
        project_id: &str,
        attachment: &AttachFieldToProject,
    ) -> Result<ProjectCustomField> {
        let _ = (project_id, attachment);
        Err(crate::error::TrackerError::InvalidInput(
            "Field attachment not supported by this backend".to_string(),
        ))
    }

    // ========== Tag Operations ==========

    /// List all available tags
    fn list_tags(&self) -> Result<Vec<IssueTag>>;

    /// Create a new tag/label
    fn create_tag(&self, tag: &CreateTag) -> Result<IssueTag> {
        let _ = tag;
        Err(crate::error::TrackerError::InvalidInput(
            "Tag creation not supported by this backend".to_string(),
        ))
    }

    /// Delete a tag/label by name
    fn delete_tag(&self, name: &str) -> Result<()> {
        let _ = name;
        Err(crate::error::TrackerError::InvalidInput(
            "Tag deletion not supported by this backend".to_string(),
        ))
    }

    /// Update a tag/label (name, color, description)
    fn update_tag(&self, current_name: &str, tag: &CreateTag) -> Result<IssueTag> {
        let _ = (current_name, tag);
        Err(crate::error::TrackerError::InvalidInput(
            "Tag update not supported by this backend".to_string(),
        ))
    }

    // ========== Link Operations ==========

    /// List all available issue link types
    fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        // Default implementation returns empty list
        Ok(Vec::new())
    }

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

/// Trait for knowledge base / wiki operations
///
/// This trait is separate from IssueTracker because not all backends may support
/// a knowledge base. YouTrack has a built-in Knowledge Base, while Jira uses
/// Confluence as a separate product.
///
/// Backends that support both issues and articles can implement both traits.
pub trait KnowledgeBase: Send + Sync {
    // ========== Article CRUD Operations ==========

    /// Get an article by its ID (database ID or readable ID like PROJ-A-1)
    fn get_article(&self, id: &str) -> Result<Article>;

    /// List articles, optionally filtered by project
    ///
    /// * `project_id` - Optional project ID or shortName to filter by
    /// * `limit` - Maximum number of articles to return
    /// * `skip` - Number of articles to skip (for pagination)
    fn list_articles(
        &self,
        project_id: Option<&str>,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<Article>>;

    /// Search articles using the backend's query language
    fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>>;

    /// Create a new article
    fn create_article(&self, article: &CreateArticle) -> Result<Article>;

    /// Update an existing article
    fn update_article(&self, id: &str, update: &UpdateArticle) -> Result<Article>;

    /// Delete an article
    fn delete_article(&self, id: &str) -> Result<()>;

    // ========== Hierarchy Operations ==========

    /// Get child articles of a parent article
    fn get_child_articles(&self, parent_id: &str) -> Result<Vec<Article>>;

    /// Move an article to a new parent (or to root if new_parent_id is None)
    fn move_article(&self, article_id: &str, new_parent_id: Option<&str>) -> Result<Article>;

    // ========== Attachment Operations ==========

    /// List attachments on an article
    fn list_article_attachments(&self, article_id: &str) -> Result<Vec<ArticleAttachment>>;

    // ========== Comment Operations ==========

    /// Get comments on an article
    fn get_article_comments(&self, article_id: &str) -> Result<Vec<Comment>>;

    /// Add a comment to an article
    fn add_article_comment(&self, article_id: &str, text: &str) -> Result<Comment>;
}
