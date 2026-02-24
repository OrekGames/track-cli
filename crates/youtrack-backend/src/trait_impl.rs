//! IssueTracker and KnowledgeBase trait implementations for YouTrackClient

use crate::client::YouTrackClient;
use crate::convert;
use crate::models::{
    AttachFieldRequest, BundleRef, CreateBundleRequest, CreateBundleValueRequest,
    CreateCustomFieldRequest, CreateIssueTagRequest, CustomFieldRef, FieldTypeRef, TagColorRequest,
};
use tracker_core::{
    Article, ArticleAttachment, AttachFieldToProject, BundleDefinition, BundleType,
    BundleValueDefinition, Comment, CreateArticle, CreateBundle, CreateBundleValue,
    CreateCustomField, CreateIssue, CreateProject, CreateTag, CustomFieldDefinition, Issue,
    IssueLink, IssueLinkType, IssueTag, IssueTracker, KnowledgeBase, Project, ProjectCustomField,
    Result, SearchResult, TrackerError, UpdateArticle, UpdateIssue, User,
};

impl IssueTracker for YouTrackClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        self.get_issue(id)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<SearchResult<Issue>> {
        // Best-effort count — don't fail the search if counting fails
        let total = self.count_issues(query).ok().flatten();

        let issues = self
            .search_issues(query, limit, skip)
            .map_err(TrackerError::from)?;
        let items: Vec<Issue> = issues.into_iter().map(Into::into).collect();

        match total {
            Some(count) => Ok(SearchResult::with_total(items, count)),
            None => Ok(SearchResult::from_items(items)),
        }
    }

    fn get_issue_count(&self, query: &str) -> Result<Option<u64>> {
        self.count_issues(query).map_err(TrackerError::from)
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        let yt_create: crate::models::CreateIssue = issue.into();
        self.create_issue(&yt_create)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        let yt_update: crate::models::UpdateIssue = update.into();
        self.update_issue(id, &yt_update)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn delete_issue(&self, id: &str) -> Result<()> {
        self.delete_issue(id).map_err(TrackerError::from)
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        self.list_projects()
            .map(|projects| projects.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        self.get_project(id)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn create_project(&self, project: &CreateProject) -> Result<Project> {
        let yt_create: crate::models::CreateProject = project.into();
        self.create_project(&yt_create)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        self.resolve_project_id(identifier)
            .map_err(TrackerError::from)
    }

    fn get_project_custom_fields(&self, project_id: &str) -> Result<Vec<ProjectCustomField>> {
        self.get_project_custom_fields(project_id)
            .map(|fields| fields.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn list_project_users(&self, project_id: &str) -> Result<Vec<User>> {
        self.list_project_users(project_id)
            .map(|users| users.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        self.list_tags()
            .map(|tags| tags.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn create_tag(&self, tag: &CreateTag) -> Result<IssueTag> {
        let request = CreateIssueTagRequest {
            name: tag.name.clone(),
            color: tag.color.as_ref().map(|hex| TagColorRequest {
                background: Some(hex.clone()),
                foreground: None,
            }),
        };
        self.create_tag(&request)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn delete_tag(&self, name: &str) -> Result<()> {
        let tags = self.list_tags().map_err(TrackerError::from)?;
        let tag = tags
            .into_iter()
            .find(|t| t.name == name)
            .ok_or_else(|| TrackerError::NotFound(format!("Tag '{}' not found", name)))?;
        self.delete_tag(&tag.id).map_err(TrackerError::from)
    }

    fn update_tag(&self, current_name: &str, tag: &CreateTag) -> Result<IssueTag> {
        let tags = self.list_tags().map_err(TrackerError::from)?;
        let existing = tags
            .into_iter()
            .find(|t| t.name == current_name)
            .ok_or_else(|| TrackerError::NotFound(format!("Tag '{}' not found", current_name)))?;
        let request = CreateIssueTagRequest {
            name: tag.name.clone(),
            color: tag.color.as_ref().map(|hex| TagColorRequest {
                background: Some(hex.clone()),
                foreground: None,
            }),
        };
        self.update_tag(&existing.id, &request)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        self.list_link_types()
            .map(|link_types| link_types.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        self.get_issue_links(issue_id)
            .map(|links| links.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn link_issues(
        &self,
        source: &str,
        target: &str,
        link_type: &str,
        direction: &str,
    ) -> Result<()> {
        self.link_issues(source, target, link_type, direction)
            .map_err(TrackerError::from)
    }

    fn link_subtask(&self, child: &str, parent: &str) -> Result<()> {
        self.link_subtask(child, parent).map_err(TrackerError::from)
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        self.add_comment(issue_id, text)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        self.get_comments(issue_id)
            .map(|comments| comments.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    // ========== Custom Field Admin Operations ==========

    fn list_custom_field_definitions(&self) -> Result<Vec<CustomFieldDefinition>> {
        self.list_custom_field_definitions()
            .map(|fields| {
                fields
                    .into_iter()
                    .map(convert::custom_field_response_to_core)
                    .collect()
            })
            .map_err(TrackerError::from)
    }

    fn create_custom_field(&self, field: &CreateCustomField) -> Result<CustomFieldDefinition> {
        let request = CreateCustomFieldRequest {
            name: field.name.clone(),
            field_type: FieldTypeRef {
                id: field.field_type.to_youtrack_id().to_string(),
            },
        };

        self.create_custom_field(&request)
            .map(convert::custom_field_response_to_core)
            .map_err(TrackerError::from)
    }

    fn list_bundles(&self, bundle_type: BundleType) -> Result<Vec<BundleDefinition>> {
        self.list_bundles(bundle_type.to_api_path())
            .map(|bundles| {
                bundles
                    .into_iter()
                    .map(convert::bundle_response_to_core)
                    .collect()
            })
            .map_err(TrackerError::from)
    }

    fn create_bundle(&self, bundle: &CreateBundle) -> Result<BundleDefinition> {
        let request = CreateBundleRequest {
            name: bundle.name.clone(),
            values: bundle
                .values
                .iter()
                .map(|v| CreateBundleValueRequest {
                    name: v.name.clone(),
                    description: v.description.clone(),
                    is_resolved: v.is_resolved,
                    ordinal: v.ordinal,
                })
                .collect(),
        };

        self.create_bundle(bundle.bundle_type.to_api_path(), &request)
            .map(convert::bundle_response_to_core)
            .map_err(TrackerError::from)
    }

    fn add_bundle_values(
        &self,
        bundle_id: &str,
        bundle_type: BundleType,
        values: &[CreateBundleValue],
    ) -> Result<Vec<BundleValueDefinition>> {
        let mut results = Vec::new();

        for value in values {
            let request = CreateBundleValueRequest {
                name: value.name.clone(),
                description: value.description.clone(),
                is_resolved: value.is_resolved,
                ordinal: value.ordinal,
            };

            let created = self
                .add_bundle_value(bundle_type.to_api_path(), bundle_id, &request)
                .map_err(TrackerError::from)?;

            results.push(convert::bundle_value_response_to_core(created));
        }

        Ok(results)
    }

    fn attach_field_to_project(
        &self,
        project_id: &str,
        attachment: &AttachFieldToProject,
    ) -> Result<ProjectCustomField> {
        // Determine the $type based on field type (default to EnumProjectCustomField if not specified)
        let type_name = attachment
            .field_type
            .map(|ft| ft.to_project_custom_field_type())
            .unwrap_or("EnumProjectCustomField")
            .to_string();

        // Build bundle reference with $type if bundle_id is provided
        let bundle = match (&attachment.bundle_id, &attachment.bundle_type) {
            (Some(id), Some(bt)) => Some(BundleRef {
                type_name: bt.to_youtrack_type().to_string(),
                id: id.clone(),
            }),
            (Some(id), None) => Some(BundleRef {
                type_name: "EnumBundle".to_string(), // Default to EnumBundle
                id: id.clone(),
            }),
            _ => None,
        };

        let request = AttachFieldRequest {
            type_name,
            field: CustomFieldRef {
                id: attachment.field_id.clone(),
            },
            bundle,
            can_be_empty: attachment.can_be_empty,
            empty_field_text: attachment.empty_field_text.clone(),
        };

        self.attach_field_to_project(project_id, &request)
            .map(convert::project_custom_field_response_to_core)
            .map_err(TrackerError::from)
    }
}

// ============================================================================
// KnowledgeBase Implementation
// ============================================================================

impl KnowledgeBase for YouTrackClient {
    fn get_article(&self, id: &str) -> Result<Article> {
        self.get_article(id)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn list_articles(
        &self,
        project_id: Option<&str>,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<Article>> {
        let result = if let Some(proj) = project_id {
            // Use search with project filter
            let query = format!("project: {}", proj);
            self.search_articles(&query, limit, skip)
        } else {
            self.list_articles(limit, skip)
        };

        result
            .map(|articles| articles.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>> {
        self.search_articles(query, limit, skip)
            .map(|articles| articles.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn create_article(&self, article: &CreateArticle) -> Result<Article> {
        let yt_create: crate::models::CreateArticle = article.into();
        self.create_article(&yt_create)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn update_article(&self, id: &str, update: &UpdateArticle) -> Result<Article> {
        let yt_update: crate::models::UpdateArticle = update.into();
        self.update_article(id, &yt_update)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn delete_article(&self, id: &str) -> Result<()> {
        self.delete_article(id).map_err(TrackerError::from)
    }

    fn get_child_articles(&self, parent_id: &str) -> Result<Vec<Article>> {
        self.get_child_articles(parent_id)
            .map(|articles| articles.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn move_article(&self, article_id: &str, new_parent_id: Option<&str>) -> Result<Article> {
        self.move_article(article_id, new_parent_id)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn list_article_attachments(&self, article_id: &str) -> Result<Vec<ArticleAttachment>> {
        self.list_article_attachments(article_id)
            .map(|attachments| attachments.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn get_article_comments(&self, article_id: &str) -> Result<Vec<Comment>> {
        self.get_article_comments(article_id)
            .map(|comments| comments.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn add_article_comment(&self, article_id: &str, text: &str) -> Result<Comment> {
        self.add_article_comment(article_id, text)
            .map(Into::into)
            .map_err(TrackerError::from)
    }
}
