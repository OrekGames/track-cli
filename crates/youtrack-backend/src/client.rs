use crate::error::{Result, YouTrackError};
use crate::models::*;
use std::time::Duration;
use ureq::Agent;

const DEFAULT_ISSUE_FIELDS: &str = "id,idReadable,summary,description,project(id,name,shortName),customFields(name,$type,value(name,login,isResolved,text)),tags(id,name),created,updated";
const DEFAULT_PROJECT_FIELDS: &str = "id,name,shortName,description";
const DEFAULT_ARTICLE_FIELDS: &str = "id,idReadable,summary,content,project(id,name,shortName),parentArticle(id,idReadable,summary),hasChildren,tags(id,name),created,updated,reporter(login,name)";

pub struct YouTrackClient {
    agent: Agent,
    base_url: String,
    token: String,
}

impl YouTrackClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();

        Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    fn handle_error(&self, err: ureq::Error) -> YouTrackError {
        match &err {
            ureq::Error::StatusCode(code) => {
                if *code == 401 {
                    YouTrackError::Unauthorized
                } else if *code == 404 {
                    YouTrackError::Api {
                        status: *code,
                        message: "Resource not found".to_string(),
                    }
                } else {
                    YouTrackError::Api {
                        status: *code,
                        message: err.to_string(),
                    }
                }
            }
            _ => YouTrackError::Http(err),
        }
    }

    pub fn get_issue(&self, id: &str) -> Result<Issue> {
        let url = format!(
            "{}/api/issues/{}?fields={}",
            self.base_url, id, DEFAULT_ISSUE_FIELDS
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let issue: Issue = response.body_mut().read_json()?;
        Ok(issue)
    }

    pub fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Issue>> {
        let url = format!(
            "{}/api/issues?query={}&fields={}&$top={}&$skip={}",
            self.base_url,
            urlencoding::encode(query),
            DEFAULT_ISSUE_FIELDS,
            limit,
            skip
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let issues: Vec<Issue> = response.body_mut().read_json()?;
        Ok(issues)
    }

    pub fn create_issue(&self, create: &CreateIssue) -> Result<Issue> {
        let url = format!(
            "{}/api/issues?fields={}",
            self.base_url, DEFAULT_ISSUE_FIELDS
        );

        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&create)
            .map_err(|e| self.handle_error(e))?;

        let issue: Issue = response.body_mut().read_json()?;
        Ok(issue)
    }

    pub fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        let url = format!(
            "{}/api/issues/{}?fields={}",
            self.base_url, id, DEFAULT_ISSUE_FIELDS
        );

        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&update)
            .map_err(|e| self.handle_error(e))?;

        let issue: Issue = response.body_mut().read_json()?;
        Ok(issue)
    }

    pub fn delete_issue(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/issues/{}", self.base_url, id);

        self.agent
            .delete(&url)
            .header("Authorization", &self.auth_header())
            .call()
            .map_err(|e| self.handle_error(e))?;

        Ok(())
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let url = format!(
            "{}/api/admin/projects?fields={}",
            self.base_url, DEFAULT_PROJECT_FIELDS
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let projects: Vec<Project> = response.body_mut().read_json()?;
        Ok(projects)
    }

    pub fn get_project(&self, id: &str) -> Result<Project> {
        let url = format!(
            "{}/api/admin/projects/{}?fields={}",
            self.base_url, id, DEFAULT_PROJECT_FIELDS
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let project: Project = response.body_mut().read_json()?;
        Ok(project)
    }

    pub fn create_project(&self, create: &CreateProject) -> Result<Project> {
        let url = format!(
            "{}/api/admin/projects?fields={}",
            self.base_url, DEFAULT_PROJECT_FIELDS
        );

        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&create)
            .map_err(|e| self.handle_error(e))?;

        let project: Project = response.body_mut().read_json()?;
        Ok(project)
    }

    /// Resolve a project identifier (shortName or ID) to internal ID.
    /// If the input looks like an internal ID (contains '-' and starts with digit), returns it as-is.
    /// Otherwise, searches projects by shortName.
    pub fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        // If it looks like an internal ID (e.g., "0-2"), return as-is
        if identifier.contains('-') && identifier.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return Ok(identifier.to_string());
        }

        // Otherwise, search by shortName
        let projects = self.list_projects()?;
        for project in projects {
            if project.short_name.eq_ignore_ascii_case(identifier) {
                return Ok(project.id);
            }
        }

        Err(YouTrackError::ProjectNotFound(identifier.to_string()))
    }

    /// Get custom fields defined for a project
    pub fn get_project_custom_fields(&self, project_id: &str) -> Result<Vec<ProjectCustomField>> {
        let url = format!(
            "{}/api/admin/projects/{}/customFields?fields=id,canBeEmpty,emptyFieldText,field(id,name,fieldType(id,presentation))",
            self.base_url, project_id
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let fields: Vec<ProjectCustomField> = response.body_mut().read_json()?;
        Ok(fields)
    }

    /// List all available issue tags
    pub fn list_tags(&self) -> Result<Vec<IssueTag>> {
        let url = format!(
            "{}/api/issueTags?fields=id,name,color(id,background,foreground),issues(id)",
            self.base_url
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let tags: Vec<IssueTag> = response.body_mut().read_json()?;
        Ok(tags)
    }

    /// List all available issue link types
    pub fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        let url = format!(
            "{}/api/issueLinkTypes?fields=id,name,sourceToTarget,targetToSource,directed",
            self.base_url
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let link_types: Vec<IssueLinkType> = response.body_mut().read_json()?;
        Ok(link_types)
    }

    /// Get links for an issue
    pub fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        let url = format!(
            "{}/api/issues/{}/links?fields=id,direction,linkType(id,name,sourceToTarget,targetToSource,directed),issues(id,idReadable,summary)",
            self.base_url, issue_id
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let links: Vec<IssueLink> = response.body_mut().read_json()?;
        Ok(links)
    }

    /// Add target issue to a link type for source issue
    /// `source_issue_id` is the issue to add the link from
    /// `link_id` is the link type id with direction suffix (e.g., "142-3t" for inward subtask)
    /// `target_issue_id` is the issue to link to
    pub fn add_issue_to_link(&self, source_issue_id: &str, link_id: &str, target_issue_id: &str) -> Result<()> {
        let url = format!(
            "{}/api/issues/{}/links/{}/issues",
            self.base_url, source_issue_id, link_id
        );

        let body = IssueRef {
            id_readable: target_issue_id.to_string(),
        };

        self.agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&body)
            .map_err(|e| self.handle_error(e))?;

        Ok(())
    }

    /// Find the link ID for a given link type name and direction
    /// Returns the link ID that can be used with add_issue_to_link
    fn find_link_id(&self, issue_id: &str, link_type_name: &str, direction: &str) -> Result<String> {
        let links = self.get_issue_links(issue_id)?;

        for link in links {
            if link.link_type.name.eq_ignore_ascii_case(link_type_name) {
                if let Some(dir) = &link.direction {
                    if dir.eq_ignore_ascii_case(direction) {
                        return Ok(link.id);
                    }
                }
            }
        }

        Err(YouTrackError::Api {
            status: 404,
            message: format!(
                "Link type '{}' with direction '{}' not found for issue '{}'",
                link_type_name, direction, issue_id
            ),
        })
    }

    /// Create a subtask link (child issue is subtask of parent issue)
    /// This creates the link from the child's perspective using "Subtask" link type
    pub fn link_subtask(&self, child_issue_id: &str, parent_issue_id: &str) -> Result<()> {
        // Find the INWARD subtask link ID for the child issue
        // INWARD means "subtask of" (child is subtask of parent)
        let link_id = self.find_link_id(child_issue_id, "Subtask", "INWARD")?;

        self.add_issue_to_link(child_issue_id, &link_id, parent_issue_id)
    }

    /// Link two issues together with the specified link type
    /// `link_type` should be one of: "Relates", "Depend", "Duplicate", "Subtask"
    /// `direction` should be "OUTWARD", "INWARD", or "BOTH" depending on the link type
    pub fn link_issues(
        &self,
        source_issue_id: &str,
        target_issue_id: &str,
        link_type: &str,
        direction: &str,
    ) -> Result<()> {
        let link_id = self.find_link_id(source_issue_id, link_type, direction)?;
        self.add_issue_to_link(source_issue_id, &link_id, target_issue_id)
    }

    /// Add a comment to an issue
    pub fn add_comment(&self, issue_id: &str, text: &str) -> Result<IssueComment> {
        let url = format!(
            "{}/api/issues/{}/comments?fields=id,text,author(login,name),created",
            self.base_url, issue_id
        );

        let comment = CreateComment {
            text: text.to_string(),
        };

        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&comment)
            .map_err(|e| self.handle_error(e))?;

        let created_comment: IssueComment = response.body_mut().read_json()?;
        Ok(created_comment)
    }

    /// Get comments for an issue
    pub fn get_comments(&self, issue_id: &str) -> Result<Vec<IssueComment>> {
        let url = format!(
            "{}/api/issues/{}/comments?fields=id,text,author(login,name),created",
            self.base_url, issue_id
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let comments: Vec<IssueComment> = response.body_mut().read_json()?;
        Ok(comments)
    }

    // ========================================================================
    // Knowledge Base / Article Operations
    // ========================================================================

    /// Get an article by ID (database ID or readable ID like PROJ-A-1)
    pub fn get_article(&self, id: &str) -> Result<Article> {
        let url = format!(
            "{}/api/articles/{}?fields={}",
            self.base_url, id, DEFAULT_ARTICLE_FIELDS
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let article: Article = response.body_mut().read_json()?;
        Ok(article)
    }

    /// List articles with pagination
    pub fn list_articles(&self, limit: usize, skip: usize) -> Result<Vec<Article>> {
        let url = format!(
            "{}/api/articles?fields={}&$top={}&$skip={}",
            self.base_url, DEFAULT_ARTICLE_FIELDS, limit, skip
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let articles: Vec<Article> = response.body_mut().read_json()?;
        Ok(articles)
    }

    /// Search articles using YouTrack query language
    pub fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>> {
        let url = format!(
            "{}/api/articles?query={}&fields={}&$top={}&$skip={}",
            self.base_url,
            urlencoding::encode(query),
            DEFAULT_ARTICLE_FIELDS,
            limit,
            skip
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let articles: Vec<Article> = response.body_mut().read_json()?;
        Ok(articles)
    }

    /// Create a new article
    pub fn create_article(&self, create: &CreateArticle) -> Result<Article> {
        let url = format!(
            "{}/api/articles?fields={}",
            self.base_url, DEFAULT_ARTICLE_FIELDS
        );

        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&create)
            .map_err(|e| self.handle_error(e))?;

        let article: Article = response.body_mut().read_json()?;
        Ok(article)
    }

    /// Update an existing article
    pub fn update_article(&self, id: &str, update: &UpdateArticle) -> Result<Article> {
        let url = format!(
            "{}/api/articles/{}?fields={}",
            self.base_url, id, DEFAULT_ARTICLE_FIELDS
        );

        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&update)
            .map_err(|e| self.handle_error(e))?;

        let article: Article = response.body_mut().read_json()?;
        Ok(article)
    }

    /// Delete an article
    pub fn delete_article(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/articles/{}", self.base_url, id);

        self.agent
            .delete(&url)
            .header("Authorization", &self.auth_header())
            .call()
            .map_err(|e| self.handle_error(e))?;

        Ok(())
    }

    /// Get child articles of a parent article
    pub fn get_child_articles(&self, parent_id: &str) -> Result<Vec<Article>> {
        let url = format!(
            "{}/api/articles/{}/childArticles?fields={}",
            self.base_url, parent_id, DEFAULT_ARTICLE_FIELDS
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let articles: Vec<Article> = response.body_mut().read_json()?;
        Ok(articles)
    }

    /// Move an article to a new parent (or to root if new_parent_id is None)
    /// This is done by updating the article's parentArticle field
    pub fn move_article(&self, article_id: &str, new_parent_id: Option<&str>) -> Result<Article> {
        let url = format!(
            "{}/api/articles/{}?fields={}",
            self.base_url, article_id, DEFAULT_ARTICLE_FIELDS
        );

        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct MoveArticle {
            #[serde(skip_serializing_if = "Option::is_none")]
            parent_article: Option<ArticleIdentifier>,
        }

        let body = MoveArticle {
            parent_article: new_parent_id.map(|id| ArticleIdentifier { id: id.to_string() }),
        };

        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&body)
            .map_err(|e| self.handle_error(e))?;

        let article: Article = response.body_mut().read_json()?;
        Ok(article)
    }

    /// List attachments on an article
    pub fn list_article_attachments(&self, article_id: &str) -> Result<Vec<ArticleAttachment>> {
        let url = format!(
            "{}/api/articles/{}/attachments?fields=id,name,size,mimeType,url,created,author(login,name)",
            self.base_url, article_id
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let attachments: Vec<ArticleAttachment> = response.body_mut().read_json()?;
        Ok(attachments)
    }

    /// Get comments on an article
    pub fn get_article_comments(&self, article_id: &str) -> Result<Vec<ArticleComment>> {
        let url = format!(
            "{}/api/articles/{}/comments?fields=id,text,author(login,name),created",
            self.base_url, article_id
        );

        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &self.auth_header())
            .header("Accept", "application/json")
            .call()
            .map_err(|e| self.handle_error(e))?;

        let comments: Vec<ArticleComment> = response.body_mut().read_json()?;
        Ok(comments)
    }

    /// Add a comment to an article
    pub fn add_article_comment(&self, article_id: &str, text: &str) -> Result<ArticleComment> {
        let url = format!(
            "{}/api/articles/{}/comments?fields=id,text,author(login,name),created",
            self.base_url, article_id
        );

        let comment = CreateArticleComment {
            text: text.to_string(),
        };

        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", &self.auth_header())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&comment)
            .map_err(|e| self.handle_error(e))?;

        let created_comment: ArticleComment = response.body_mut().read_json()?;
        Ok(created_comment)
    }
}
