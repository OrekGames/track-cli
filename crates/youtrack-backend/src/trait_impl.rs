//! IssueTracker and KnowledgeBase trait implementations for YouTrackClient

use crate::client::YouTrackClient;
use tracker_core::{
    Article, ArticleAttachment, Comment, CreateArticle, CreateIssue, CreateProject, Issue,
    IssueLink, IssueTag, IssueTracker, KnowledgeBase, Project, ProjectCustomField, Result,
    TrackerError, UpdateArticle, UpdateIssue,
};

impl IssueTracker for YouTrackClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        self.get_issue(id)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Issue>> {
        self.search_issues(query, limit, skip)
            .map(|issues| issues.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
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

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        self.list_tags()
            .map(|tags| tags.into_iter().map(Into::into).collect())
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
        self.link_subtask(child, parent)
            .map_err(TrackerError::from)
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
