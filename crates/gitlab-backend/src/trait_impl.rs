//! Implementation of tracker-core traits for GitLabClient

use tracker_core::{
    Article, ArticleAttachment, Comment, CreateArticle, CreateIssue, CreateProject, CreateTag,
    Issue, IssueLink, IssueLinkType, IssueTag, IssueTracker, KnowledgeBase, Project,
    ProjectCustomField, Result, SearchResult, TrackerError, UpdateArticle, UpdateIssue, User,
};

use crate::client::GitLabClient;
use crate::convert::{
    convert_query_to_gitlab_params, get_gitlab_link_types, get_standard_custom_fields,
    gitlab_issue_to_core, gitlab_link_to_core, map_link_type,
};
use crate::models::{
    CreateGitLabIssue, CreateGitLabIssueLink, CreateGitLabLabel, UpdateGitLabIssue,
    UpdateGitLabLabel,
};

/// Parse an issue IID from a string, stripping an optional leading `#`.
fn parse_issue_iid(id: &str) -> std::result::Result<u64, TrackerError> {
    let stripped = id.strip_prefix('#').unwrap_or(id);
    stripped.parse::<u64>().map_err(|_| {
        TrackerError::InvalidInput(format!(
            "Invalid GitLab issue IID '{}': must be a number (optionally prefixed with #)",
            id
        ))
    })
}

impl IssueTracker for GitLabClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        let iid = parse_issue_iid(id)?;
        let project_id = self.project_id_str();
        Ok(gitlab_issue_to_core(self.get_issue(iid)?, &project_id))
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<SearchResult<Issue>> {
        let (search_text, state, labels) = convert_query_to_gitlab_params(query);
        let page = if limit > 0 { skip / limit + 1 } else { 1 };
        let project_id = self.project_id_str();

        // Use combined methods that read X-Total from the search response itself
        let (issues, total) = if search_text.is_empty() {
            self.list_issues_with_total(state.as_deref(), limit, page)?
        } else {
            self.search_issues_with_total(
                &search_text,
                state.as_deref(),
                labels.as_deref(),
                limit,
                page,
            )?
        };

        let items: Vec<Issue> = issues
            .into_iter()
            .map(|i| gitlab_issue_to_core(i, &project_id))
            .collect();

        match total {
            Some(count) => Ok(SearchResult::with_total(items, count)),
            None => Ok(SearchResult::from_items(items)),
        }
    }

    fn get_issue_count(&self, query: &str) -> Result<Option<u64>> {
        let (search_text, state, _labels) = convert_query_to_gitlab_params(query);
        Ok(self.count_issues_by_query(&search_text, state.as_deref())?)
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        if issue.parent.is_some() {
            return Err(TrackerError::InvalidInput(
                "GitLab does not support a parent field. Use issue links instead.".to_string(),
            ));
        }

        let labels = if issue.tags.is_empty() {
            None
        } else {
            Some(issue.tags.join(","))
        };

        let create = CreateGitLabIssue {
            title: issue.summary.clone(),
            description: issue.description.clone(),
            labels,
            assignee_ids: None,
            milestone_id: None,
        };

        let project_id = self.project_id_str();
        Ok(gitlab_issue_to_core(
            self.create_issue(&create)?,
            &project_id,
        ))
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        if update.parent.is_some() {
            return Err(TrackerError::InvalidInput(
                "GitLab does not support a parent field. Use issue links instead.".to_string(),
            ));
        }

        let iid = parse_issue_iid(id)?;

        // Check for state changes in custom_fields
        let state_event = update.custom_fields.iter().find_map(|cf| match cf {
            tracker_core::CustomFieldUpdate::State { name, value }
                if name.to_lowercase() == "status" || name.to_lowercase() == "state" =>
            {
                match value.to_lowercase().as_str() {
                    "closed" | "resolved" | "done" => Some("close".to_string()),
                    "open" | "opened" | "reopen" | "reopened" => Some("reopen".to_string()),
                    _ => None,
                }
            }
            _ => None,
        });

        let labels = if update.tags.is_empty() {
            None
        } else {
            Some(update.tags.join(","))
        };

        let gitlab_update = UpdateGitLabIssue {
            title: update.summary.clone(),
            description: update.description.clone(),
            labels,
            state_event,
            assignee_ids: None,
            milestone_id: None,
        };

        let project_id = self.project_id_str();
        Ok(gitlab_issue_to_core(
            self.update_issue(iid, &gitlab_update)?,
            &project_id,
        ))
    }

    fn delete_issue(&self, id: &str) -> Result<()> {
        let iid = parse_issue_iid(id)?;
        Ok(self.delete_issue(iid)?)
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        Ok(self.list_projects()?.into_iter().map(Into::into).collect())
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        Ok(self.get_project(id)?.into())
    }

    fn create_project(&self, _project: &CreateProject) -> Result<Project> {
        Err(TrackerError::InvalidInput(
            "Creating projects via API is not supported. Please use the GitLab web interface."
                .to_string(),
        ))
    }

    fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        Ok(identifier.to_string())
    }

    fn get_project_custom_fields(&self, _project_id: &str) -> Result<Vec<ProjectCustomField>> {
        Ok(get_standard_custom_fields())
    }

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        Ok(self.list_labels()?.into_iter().map(Into::into).collect())
    }

    fn create_tag(&self, tag: &CreateTag) -> Result<IssueTag> {
        let color = tag.color.clone().unwrap_or_else(|| "#ededed".to_string());
        let label = CreateGitLabLabel {
            name: tag.name.clone(),
            color,
            description: tag.description.clone(),
        };
        Ok(self.create_label(&label)?.into())
    }

    fn delete_tag(&self, name: &str) -> Result<()> {
        let labels = self.list_labels()?;
        let label = labels
            .into_iter()
            .find(|l| l.name == name)
            .ok_or_else(|| TrackerError::InvalidInput(format!("Tag '{}' not found", name)))?;
        Ok(self.delete_label(label.id)?)
    }

    fn update_tag(&self, current_name: &str, tag: &CreateTag) -> Result<IssueTag> {
        let labels = self.list_labels()?;
        let label = labels
            .into_iter()
            .find(|l| l.name == current_name)
            .ok_or_else(|| {
                TrackerError::InvalidInput(format!("Tag '{}' not found", current_name))
            })?;
        let update = UpdateGitLabLabel {
            new_name: Some(tag.name.clone()),
            color: tag.color.clone(),
            description: tag.description.clone(),
        };
        Ok(self.update_label(label.id, &update)?.into())
    }

    fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        Ok(get_gitlab_link_types())
    }

    fn list_project_users(&self, _project_id: &str) -> Result<Vec<User>> {
        Ok(self
            .list_project_members()?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        let iid = parse_issue_iid(issue_id)?;
        Ok(self
            .get_issue_links(iid)?
            .into_iter()
            .map(|l| gitlab_link_to_core(l, iid))
            .collect())
    }

    fn link_issues(
        &self,
        source: &str,
        target: &str,
        link_type: &str,
        _direction: &str,
    ) -> Result<()> {
        let source_iid = parse_issue_iid(source)?;
        let target_iid = parse_issue_iid(target)?;
        let gitlab_link_type = map_link_type(link_type);

        let project_id = self.project_id_str();
        let link = CreateGitLabIssueLink {
            target_project_id: project_id,
            target_issue_iid: target_iid,
            link_type: gitlab_link_type.to_string(),
        };

        Ok(self.create_issue_link(source_iid, &link)?)
    }

    fn link_subtask(&self, _child: &str, _parent: &str) -> Result<()> {
        Err(TrackerError::InvalidInput(
            "GitLab does not support native subtask relationships. Use issue links instead."
                .to_string(),
        ))
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        let iid = parse_issue_iid(issue_id)?;
        Ok(self.add_note(iid, text)?.into())
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        let iid = parse_issue_iid(issue_id)?;
        Ok(self.get_notes(iid)?.into_iter().map(Into::into).collect())
    }
}

// ==================== KnowledgeBase stub ====================
// GitLab does not support a knowledge base through this client.
// main.rs calls `run_with_client(&client, &client, ...)` so GitLabClient
// must implement KnowledgeBase.

impl KnowledgeBase for GitLabClient {
    fn get_article(&self, _id: &str) -> Result<Article> {
        Err(TrackerError::InvalidInput(
            "GitLab backend does not support articles/knowledge base".to_string(),
        ))
    }

    fn list_articles(
        &self,
        _project_id: Option<&str>,
        _limit: usize,
        _skip: usize,
    ) -> Result<Vec<Article>> {
        Ok(Vec::new())
    }

    fn search_articles(&self, _query: &str, _limit: usize, _skip: usize) -> Result<Vec<Article>> {
        Ok(Vec::new())
    }

    fn create_article(&self, _article: &CreateArticle) -> Result<Article> {
        Err(TrackerError::InvalidInput(
            "GitLab backend does not support articles/knowledge base".to_string(),
        ))
    }

    fn update_article(&self, _id: &str, _update: &UpdateArticle) -> Result<Article> {
        Err(TrackerError::InvalidInput(
            "GitLab backend does not support articles/knowledge base".to_string(),
        ))
    }

    fn delete_article(&self, _id: &str) -> Result<()> {
        Err(TrackerError::InvalidInput(
            "GitLab backend does not support articles/knowledge base".to_string(),
        ))
    }

    fn get_child_articles(&self, _parent_id: &str) -> Result<Vec<Article>> {
        Ok(Vec::new())
    }

    fn move_article(&self, _article_id: &str, _new_parent_id: Option<&str>) -> Result<Article> {
        Err(TrackerError::InvalidInput(
            "GitLab backend does not support articles/knowledge base".to_string(),
        ))
    }

    fn list_article_attachments(&self, _article_id: &str) -> Result<Vec<ArticleAttachment>> {
        Ok(Vec::new())
    }

    fn get_article_comments(&self, _article_id: &str) -> Result<Vec<Comment>> {
        Ok(Vec::new())
    }

    fn add_article_comment(&self, _article_id: &str, _text: &str) -> Result<Comment> {
        Err(TrackerError::InvalidInput(
            "GitLab backend does not support articles/knowledge base".to_string(),
        ))
    }
}
