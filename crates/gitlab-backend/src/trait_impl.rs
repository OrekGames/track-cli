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
        self.get_issue(iid)
            .map(|i| gitlab_issue_to_core(i, &project_id))
            .map_err(TrackerError::from)
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<SearchResult<Issue>> {
        let (search_text, state, labels) = convert_query_to_gitlab_params(query);
        let page = if limit > 0 { skip / limit + 1 } else { 1 };
        let project_id = self.project_id_str();

        // Use combined methods that read X-Total from the search response itself
        let (issues, total) = if search_text.is_empty() {
            self.list_issues_with_total(state.as_deref(), limit, page)
                .map_err(TrackerError::from)?
        } else {
            self.search_issues_with_total(
                &search_text,
                state.as_deref(),
                labels.as_deref(),
                limit,
                page,
            )
            .map_err(TrackerError::from)?
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
        self.count_issues_by_query(&search_text, state.as_deref())
            .map_err(TrackerError::from)
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
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
        self.create_issue(&create)
            .map(|i| gitlab_issue_to_core(i, &project_id))
            .map_err(TrackerError::from)
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
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
        self.update_issue(iid, &gitlab_update)
            .map(|i| gitlab_issue_to_core(i, &project_id))
            .map_err(TrackerError::from)
    }

    fn delete_issue(&self, id: &str) -> Result<()> {
        let iid = parse_issue_iid(id)?;
        self.delete_issue(iid).map_err(TrackerError::from)
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        self.list_projects()
            .map(|ps| ps.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        self.get_project(id)
            .map(Into::into)
            .map_err(TrackerError::from)
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
        self.list_labels()
            .map(|labels| labels.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn create_tag(&self, tag: &CreateTag) -> Result<IssueTag> {
        let color = tag.color.clone().unwrap_or_else(|| "#ededed".to_string());
        let label = CreateGitLabLabel {
            name: tag.name.clone(),
            color,
            description: tag.description.clone(),
        };
        self.create_label(&label)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn delete_tag(&self, name: &str) -> Result<()> {
        let labels = self.list_labels().map_err(TrackerError::from)?;
        let label = labels
            .into_iter()
            .find(|l| l.name == name)
            .ok_or_else(|| TrackerError::InvalidInput(format!("Tag '{}' not found", name)))?;
        self.delete_label(label.id).map_err(TrackerError::from)
    }

    fn update_tag(&self, current_name: &str, tag: &CreateTag) -> Result<IssueTag> {
        let labels = self.list_labels().map_err(TrackerError::from)?;
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
        self.update_label(label.id, &update)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        Ok(get_gitlab_link_types())
    }

    fn list_project_users(&self, _project_id: &str) -> Result<Vec<User>> {
        self.list_project_members()
            .map(|members| members.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        let iid = parse_issue_iid(issue_id)?;
        self.get_issue_links(iid)
            .map(|links| {
                links
                    .into_iter()
                    .map(|l| gitlab_link_to_core(l, iid))
                    .collect()
            })
            .map_err(TrackerError::from)
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

        self.create_issue_link(source_iid, &link)
            .map_err(TrackerError::from)
    }

    fn link_subtask(&self, _child: &str, _parent: &str) -> Result<()> {
        Err(TrackerError::InvalidInput(
            "GitLab does not support native subtask relationships. Use issue links instead."
                .to_string(),
        ))
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        let iid = parse_issue_iid(issue_id)?;
        self.add_note(iid, text)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        let iid = parse_issue_iid(issue_id)?;
        self.get_notes(iid)
            .map(|notes| notes.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
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
