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

    /// Fetch all issues matching `query`, auto-paginating up to `max_results`.
    ///
    /// The default implementation pages through [`Self::search_issues`] in
    /// offset windows of 100, deduplicating by issue id and failing with
    /// [`crate::TrackerError::PaginationStalled`] if a full page yields no
    /// new issues. Backends with cursor-based search APIs (Jira Cloud
    /// `/search/jql`, Linear) should override this with a native cursor walk
    /// so a full scan costs O(pages) requests instead of O(pages²) through
    /// offset emulation.
    fn search_all_issues(&self, query: &str, max_results: usize) -> Result<Vec<Issue>> {
        crate::pagination::fetch_all_pages_keyed(
            |offset, limit| self.search_issues(query, limit, offset).map(|r| r.items),
            100,
            max_results,
            |issue: &Issue| issue.id.clone(),
        )
    }

    /// Create a new issue
    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue>;

    /// Update an existing issue
    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue>;

    /// Delete an issue
    fn delete_issue(&self, id: &str) -> Result<()>;

    // ========== Attachment Operations ==========

    /// List attachments on an issue.
    fn list_issue_attachments(&self, issue_id: &str) -> Result<Vec<IssueAttachment>> {
        let _ = issue_id;
        Err(crate::error::TrackerError::InvalidInput(
            "Issue attachments are not supported by this backend".to_string(),
        ))
    }

    /// Upload one or more files to an issue.
    fn add_issue_attachment(
        &self,
        issue_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<IssueAttachment>> {
        let _ = (issue_id, upload);
        Err(crate::error::TrackerError::InvalidInput(
            "Issue attachment upload is not supported by this backend".to_string(),
        ))
    }

    /// Add a comment with one or more native attachments to an issue.
    fn add_issue_comment_attachment(
        &self,
        issue_id: &str,
        text: &str,
        upload: &AttachmentUpload,
    ) -> Result<Comment> {
        let _ = (issue_id, text, upload);
        Err(crate::error::TrackerError::InvalidInput(
            "Issue comment attachment upload is not supported by this backend".to_string(),
        ))
    }

    /// Whether this backend supports native attachments on issue comments.
    fn supports_issue_comment_attachments(&self) -> bool {
        false
    }

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

    /// Remove a link by its link ID (as returned by `get_issue_links`).
    fn unlink_issues(&self, source: &str, link_id: &str) -> Result<()> {
        let _ = (source, link_id);
        Err(crate::error::TrackerError::InvalidInput(
            "Unlinking issues is not supported by this backend".to_string(),
        ))
    }

    // ========== Comment Operations ==========

    /// Add a comment to an issue
    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment>;

    /// Get comments for an issue
    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>>;

    /// Get a page of comments for an issue using offset pagination.
    ///
    /// Backends with page-based or cursor-based APIs should adapt those native
    /// mechanisms so callers can rely on `limit` and `skip` as an offset window.
    fn get_comments_page(&self, issue_id: &str, limit: usize, skip: usize) -> Result<Vec<Comment>> {
        Ok(self
            .get_comments(issue_id)?
            .into_iter()
            .skip(skip)
            .take(limit)
            .collect())
    }
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

    /// Upload one or more files to an article.
    fn add_article_attachment(
        &self,
        article_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<ArticleAttachment>> {
        let _ = (article_id, upload);
        Err(crate::error::TrackerError::InvalidInput(
            "Article attachment upload is not supported by this backend".to_string(),
        ))
    }

    // ========== Comment Operations ==========

    /// Get comments on an article
    fn get_article_comments(&self, article_id: &str) -> Result<Vec<Comment>>;

    /// Add a comment to an article
    fn add_article_comment(&self, article_id: &str, text: &str) -> Result<Comment>;

    /// Add a comment with one or more native attachments to an article.
    fn add_article_comment_attachment(
        &self,
        article_id: &str,
        text: &str,
        upload: &AttachmentUpload,
    ) -> Result<Comment> {
        let _ = (article_id, text, upload);
        Err(crate::error::TrackerError::InvalidInput(
            "Article comment attachment upload is not supported by this backend".to_string(),
        ))
    }

    /// Whether this backend supports native attachments on article comments.
    fn supports_article_comment_attachments(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TrackerError;
    use std::sync::Mutex;

    fn test_issue(n: usize) -> Issue {
        Issue {
            id: format!("id-{n}"),
            id_readable: format!("TEST-{n}"),
            summary: format!("Issue {n}"),
            description: None,
            project: ProjectRef {
                id: "proj-1".to_string(),
                name: None,
                short_name: None,
            },
            custom_fields: Vec::new(),
            tags: Vec::new(),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            resolved: None,
        }
    }

    /// Offset-window search stub. `ignore_skip` simulates the issue #252
    /// failure mode where the server ignores the offset parameter.
    struct StubTracker {
        issues: Vec<Issue>,
        ignore_skip: bool,
        calls: Mutex<Vec<(usize, usize)>>,
    }

    impl StubTracker {
        fn new(count: usize, ignore_skip: bool) -> Self {
            Self {
                issues: (1..=count).map(test_issue).collect(),
                ignore_skip,
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl IssueTracker for StubTracker {
        fn get_issue(&self, _: &str) -> Result<Issue> {
            unimplemented!()
        }
        fn search_issues(
            &self,
            _query: &str,
            limit: usize,
            skip: usize,
        ) -> Result<SearchResult<Issue>> {
            self.calls.lock().unwrap().push((skip, limit));
            let skip = if self.ignore_skip { 0 } else { skip };
            let items = self.issues.iter().skip(skip).take(limit).cloned().collect();
            Ok(SearchResult::from_items(items))
        }
        fn create_issue(&self, _: &CreateIssue) -> Result<Issue> {
            unimplemented!()
        }
        fn update_issue(&self, _: &str, _: &UpdateIssue) -> Result<Issue> {
            unimplemented!()
        }
        fn delete_issue(&self, _: &str) -> Result<()> {
            unimplemented!()
        }
        fn list_projects(&self) -> Result<Vec<Project>> {
            unimplemented!()
        }
        fn get_project(&self, _: &str) -> Result<Project> {
            unimplemented!()
        }
        fn create_project(&self, _: &CreateProject) -> Result<Project> {
            unimplemented!()
        }
        fn resolve_project_id(&self, _: &str) -> Result<String> {
            unimplemented!()
        }
        fn get_project_custom_fields(&self, _: &str) -> Result<Vec<ProjectCustomField>> {
            unimplemented!()
        }
        fn list_tags(&self) -> Result<Vec<IssueTag>> {
            unimplemented!()
        }
        fn get_issue_links(&self, _: &str) -> Result<Vec<IssueLink>> {
            unimplemented!()
        }
        fn link_issues(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
        fn link_subtask(&self, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
        fn add_comment(&self, _: &str, _: &str) -> Result<Comment> {
            unimplemented!()
        }
        fn get_comments(&self, _: &str) -> Result<Vec<Comment>> {
            unimplemented!()
        }
    }

    #[test]
    fn search_all_issues_default_pages_with_offset() {
        let tracker = StubTracker::new(250, false);
        let result = tracker.search_all_issues("query", 1000).unwrap();
        assert_eq!(result.len(), 250);
        assert_eq!(result[0].id, "id-1");
        assert_eq!(result[249].id, "id-250");
        // Same request sequence the old fetch_all_pages closure produced
        assert_eq!(
            *tracker.calls.lock().unwrap(),
            vec![(0, 100), (100, 100), (200, 100)]
        );
    }

    #[test]
    fn search_all_issues_default_respects_max_results() {
        let tracker = StubTracker::new(250, false);
        let result = tracker.search_all_issues("query", 150).unwrap();
        assert_eq!(result.len(), 150);
        assert_eq!(*tracker.calls.lock().unwrap(), vec![(0, 100), (100, 50)]);
    }

    #[test]
    fn search_all_issues_default_errors_when_backend_ignores_skip() {
        // The issue #252 regression: offset ignored => same page forever.
        // Must fail loudly instead of returning truncated data.
        let tracker = StubTracker::new(250, true);
        let result = tracker.search_all_issues("query", 1000);
        assert!(matches!(result, Err(TrackerError::PaginationStalled(_))));
        assert_eq!(tracker.calls.lock().unwrap().len(), 2);
    }
}
