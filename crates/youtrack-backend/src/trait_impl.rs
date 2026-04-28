//! IssueTracker and KnowledgeBase trait implementations for YouTrackClient

use crate::client::YouTrackClient;
use crate::convert;
use crate::models::{
    AttachFieldRequest, BundleRef, CreateBundleRequest, CreateBundleValueRequest,
    CreateCustomFieldRequest, CreateIssueTagRequest, CustomFieldRef, FieldTypeRef, TagColorRequest,
};
use tracker_core::{
    Article, ArticleAttachment, AttachFieldToProject, AttachmentUpload, BundleDefinition,
    BundleType, BundleValueDefinition, Comment, CreateArticle, CreateBundle, CreateBundleValue,
    CreateCustomField, CreateIssue, CreateProject, CreateTag, CustomFieldDefinition, Issue,
    IssueAttachment, IssueLink, IssueLinkType, IssueTag, IssueTracker, KnowledgeBase, Project,
    ProjectCustomField, Result, SearchResult, TrackerError, UpdateArticle, UpdateIssue, User,
};

impl IssueTracker for YouTrackClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        Ok(self.get_issue(id)?.into())
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<SearchResult<Issue>> {
        // Best-effort count — don't fail the search if counting fails
        let total = self.count_issues(query).ok().flatten();

        let issues = self.search_issues(query, limit, skip)?;
        let items: Vec<Issue> = issues.into_iter().map(Into::into).collect();

        match total {
            Some(count) => Ok(SearchResult::with_total(items, count)),
            None => Ok(SearchResult::from_items(items)),
        }
    }

    fn get_issue_count(&self, query: &str) -> Result<Option<u64>> {
        Ok(self.count_issues(query)?)
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        let yt_create: crate::models::CreateIssue = issue.into();
        let created: Issue = self.create_issue(&yt_create)?.into();
        if let Some(ref parent_id) = issue.parent {
            self.link_subtask(&created.id_readable, parent_id)?;
        }
        Ok(created)
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        let yt_update: crate::models::UpdateIssue = update.into();
        let updated: Issue = self.update_issue(id, &yt_update)?.into();
        if let Some(ref parent_id) = update.parent {
            self.link_subtask(id, parent_id)?;
        }
        Ok(updated)
    }

    fn delete_issue(&self, id: &str) -> Result<()> {
        Ok(self.delete_issue(id)?)
    }

    fn list_issue_attachments(&self, issue_id: &str) -> Result<Vec<IssueAttachment>> {
        Ok(self
            .list_issue_attachments(issue_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn add_issue_attachment(
        &self,
        issue_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<IssueAttachment>> {
        Ok(self
            .add_issue_attachments(issue_id, upload)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        Ok(self.list_projects()?.into_iter().map(Into::into).collect())
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        Ok(self.get_project(id)?.into())
    }

    fn create_project(&self, project: &CreateProject) -> Result<Project> {
        let yt_create: crate::models::CreateProject = project.into();
        Ok(self.create_project(&yt_create)?.into())
    }

    fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        Ok(self.resolve_project_id(identifier)?)
    }

    fn get_project_custom_fields(&self, project_id: &str) -> Result<Vec<ProjectCustomField>> {
        Ok(self
            .get_project_custom_fields(project_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn list_project_users(&self, project_id: &str) -> Result<Vec<User>> {
        Ok(self
            .list_project_users(project_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        Ok(self.list_tags()?.into_iter().map(Into::into).collect())
    }

    fn create_tag(&self, tag: &CreateTag) -> Result<IssueTag> {
        let request = CreateIssueTagRequest {
            name: tag.name.clone(),
            color: tag.color.as_ref().map(|hex| TagColorRequest {
                background: Some(hex.clone()),
                foreground: None,
            }),
        };
        Ok(self.create_tag(&request)?.into())
    }

    fn delete_tag(&self, name: &str) -> Result<()> {
        let tags = self.list_tags()?;
        let tag = tags
            .into_iter()
            .find(|t| t.name == name)
            .ok_or_else(|| TrackerError::NotFound(format!("Tag '{}' not found", name)))?;
        Ok(self.delete_tag(&tag.id)?)
    }

    fn update_tag(&self, current_name: &str, tag: &CreateTag) -> Result<IssueTag> {
        let tags = self.list_tags()?;
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
        Ok(self.update_tag(&existing.id, &request)?.into())
    }

    fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        Ok(self
            .list_link_types()?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        Ok(self
            .get_issue_links(issue_id)?
            .into_iter()
            .flat_map(convert::flatten_youtrack_link)
            .collect())
    }

    fn unlink_issues(&self, source: &str, link_id: &str) -> Result<()> {
        // Composite ID format: "bucket_id/TARGET_ID"
        let (bucket_id, target_id) = link_id.split_once('/').ok_or_else(|| {
            TrackerError::InvalidInput(format!(
                "Invalid YouTrack link ID '{}': expected format 'bucket_id/TARGET_ID'",
                link_id
            ))
        })?;
        Ok(self.remove_issue_from_link(source, bucket_id, target_id)?)
    }

    fn link_issues(
        &self,
        source: &str,
        target: &str,
        link_type: &str,
        direction: &str,
    ) -> Result<()> {
        Ok(self.link_issues(source, target, link_type, direction)?)
    }

    fn link_subtask(&self, child: &str, parent: &str) -> Result<()> {
        Ok(self.link_subtask(child, parent)?)
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        Ok(self.add_comment(issue_id, text)?.into())
    }

    fn add_issue_comment_attachment(
        &self,
        issue_id: &str,
        text: &str,
        upload: &AttachmentUpload,
    ) -> Result<Comment> {
        let comment = self.add_comment(issue_id, text)?;
        self.add_issue_comment_attachments(issue_id, &comment.id, upload)?;
        Ok(comment.into())
    }

    fn supports_issue_comment_attachments(&self) -> bool {
        true
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        Ok(self
            .get_comments(issue_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn get_comments_page(&self, issue_id: &str, limit: usize, skip: usize) -> Result<Vec<Comment>> {
        Ok(self
            .get_comments_page(issue_id, limit, skip)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    // ========== Custom Field Admin Operations ==========

    fn list_custom_field_definitions(&self) -> Result<Vec<CustomFieldDefinition>> {
        Ok(self
            .list_custom_field_definitions()?
            .into_iter()
            .map(convert::custom_field_response_to_core)
            .collect())
    }

    fn create_custom_field(&self, field: &CreateCustomField) -> Result<CustomFieldDefinition> {
        let request = CreateCustomFieldRequest {
            name: field.name.clone(),
            field_type: FieldTypeRef {
                id: field.field_type.to_youtrack_id().to_string(),
            },
        };
        Ok(convert::custom_field_response_to_core(
            self.create_custom_field(&request)?,
        ))
    }

    fn list_bundles(&self, bundle_type: BundleType) -> Result<Vec<BundleDefinition>> {
        Ok(self
            .list_bundles(bundle_type.to_api_path())?
            .into_iter()
            .map(convert::bundle_response_to_core)
            .collect())
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
        Ok(convert::bundle_response_to_core(self.create_bundle(
            bundle.bundle_type.to_api_path(),
            &request,
        )?))
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

            let created = self.add_bundle_value(bundle_type.to_api_path(), bundle_id, &request)?;

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

        Ok(convert::project_custom_field_response_to_core(
            self.attach_field_to_project(project_id, &request)?,
        ))
    }
}

// ============================================================================
// KnowledgeBase Implementation
// ============================================================================

impl KnowledgeBase for YouTrackClient {
    fn get_article(&self, id: &str) -> Result<Article> {
        Ok(self.get_article(id)?.into())
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

        Ok(result?.into_iter().map(Into::into).collect())
    }

    fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>> {
        Ok(self
            .search_articles(query, limit, skip)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn create_article(&self, article: &CreateArticle) -> Result<Article> {
        let yt_create: crate::models::CreateArticle = article.into();
        Ok(self.create_article(&yt_create)?.into())
    }

    fn update_article(&self, id: &str, update: &UpdateArticle) -> Result<Article> {
        let yt_update: crate::models::UpdateArticle = update.into();
        Ok(self.update_article(id, &yt_update)?.into())
    }

    fn delete_article(&self, id: &str) -> Result<()> {
        Ok(self.delete_article(id)?)
    }

    fn get_child_articles(&self, parent_id: &str) -> Result<Vec<Article>> {
        Ok(self
            .get_child_articles(parent_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn move_article(&self, article_id: &str, new_parent_id: Option<&str>) -> Result<Article> {
        Ok(self.move_article(article_id, new_parent_id)?.into())
    }

    fn list_article_attachments(&self, article_id: &str) -> Result<Vec<ArticleAttachment>> {
        Ok(self
            .list_article_attachments(article_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn get_article_comments(&self, article_id: &str) -> Result<Vec<Comment>> {
        Ok(self
            .get_article_comments(article_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn add_article_comment(&self, article_id: &str, text: &str) -> Result<Comment> {
        Ok(self.add_article_comment(article_id, text)?.into())
    }
}
