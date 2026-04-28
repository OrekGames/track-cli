//! Implementation of tracker-core traits for GitHubClient

use tracker_core::{
    Article, ArticleAttachment, ArticleRef, AttachmentUpload, Comment, CommentAuthor,
    CreateArticle, CreateIssue, CreateProject, CreateTag, Issue, IssueAttachment, IssueLink,
    IssueTag, IssueTracker, KnowledgeBase, Project, ProjectCustomField, ProjectRef, Result,
    SearchResult, Tag, TrackerError, UpdateArticle, UpdateIssue,
};

use crate::client::GitHubClient;
use crate::convert::{
    convert_query_to_github, create_issue_from_core, get_standard_custom_fields,
    github_issue_to_core, update_issue_from_core,
};
use crate::wiki::WikiPage;

/// Parse an issue number from a string identifier.
///
/// Accepts:
/// - A raw number: "42"
/// - An owner/repo#number format: "owner/repo#42"
fn parse_issue_number(id: &str) -> std::result::Result<u64, TrackerError> {
    // Try parsing as a raw number first
    if let Ok(n) = id.parse::<u64>() {
        return Ok(n);
    }

    // Try owner/repo#number format
    if let Some(hash_pos) = id.rfind('#') {
        let number_str = &id[hash_pos + 1..];
        if let Ok(n) = number_str.parse::<u64>() {
            return Ok(n);
        }
    }

    Err(TrackerError::InvalidInput(format!(
        "Invalid GitHub issue identifier: '{}'. Expected a number or 'owner/repo#number' format.",
        id
    )))
}

impl IssueTracker for GitHubClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        let number = parse_issue_number(id)?;
        let issue = self.get_issue(number)?;

        // If this is a PR, report as not found
        if issue.is_pull_request() {
            return Err(TrackerError::IssueNotFound(id.to_string()));
        }

        Ok(github_issue_to_core(issue, self.owner(), self.repo()))
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<SearchResult<Issue>> {
        let github_query = convert_query_to_github(query);
        let owner = self.owner().to_string();
        let repo = self.repo().to_string();

        if limit == 0 {
            return Ok(SearchResult::from_items(Vec::new()));
        }

        let per_page = 100;
        let mut page = (skip / per_page) + 1;
        let mut page_offset = skip % per_page;
        let mut items = Vec::new();
        let mut total = None;

        while items.len() < limit {
            let result = self.search_issues(&github_query, per_page, page)?;
            if total.is_none() {
                total = Some(result.total_count);
            }

            let page_len = result.items.len();
            let remaining = limit - items.len();
            items.extend(
                result
                    .items
                    .into_iter()
                    .filter(|i| !i.is_pull_request())
                    .skip(page_offset)
                    .take(remaining)
                    .map(|i| github_issue_to_core(i, &owner, &repo)),
            );

            if page_len < per_page {
                break;
            }

            page += 1;
            page_offset = 0;
        }

        if let Some(total) = total {
            Ok(SearchResult::with_total(items, total))
        } else {
            Ok(SearchResult::from_items(items))
        }
    }

    fn get_issue_count(&self, query: &str) -> Result<Option<u64>> {
        Ok(Some(self.count_issues(query)?))
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        let github_issue = create_issue_from_core(issue);
        let created = self.create_issue(&github_issue)?;

        // If a parent was requested, add as sub-issue via the sub-issues API
        if let Some(ref parent_id) = issue.parent {
            let parent_number = parse_issue_number(parent_id)?;
            self.add_sub_issue(parent_number, created.id)?;
        }

        Ok(github_issue_to_core(created, self.owner(), self.repo()))
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        let number = parse_issue_number(id)?;
        let github_update = update_issue_from_core(update);
        let updated = self.update_issue(number, &github_update)?;

        // If a parent was requested, add as sub-issue via the sub-issues API
        if let Some(ref parent_id) = update.parent {
            let parent_number = parse_issue_number(parent_id)?;
            self.add_sub_issue(parent_number, updated.id)?;
        }

        Ok(github_issue_to_core(updated, self.owner(), self.repo()))
    }

    fn delete_issue(&self, _id: &str) -> Result<()> {
        Err(TrackerError::InvalidInput(
            "GitHub does not support deleting issues. Use update to close them instead."
                .to_string(),
        ))
    }

    fn list_issue_attachments(&self, _issue_id: &str) -> Result<Vec<IssueAttachment>> {
        Err(TrackerError::InvalidInput(
            "GitHub Issues does not expose a public REST API for listing issue file attachments."
                .to_string(),
        ))
    }

    fn add_issue_attachment(
        &self,
        _issue_id: &str,
        _upload: &AttachmentUpload,
    ) -> Result<Vec<IssueAttachment>> {
        Err(TrackerError::InvalidInput(
            "GitHub Issues does not expose a public REST API for uploading issue file attachments."
                .to_string(),
        ))
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        Ok(self.list_repos()?.into_iter().map(Into::into).collect())
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        // Parse "owner/repo" format
        let (owner, repo) = if let Some(slash_pos) = id.find('/') {
            (&id[..slash_pos], &id[slash_pos + 1..])
        } else {
            // If no slash, assume same owner, and id is the repo name
            (self.owner(), id)
        };

        self.get_repo(owner, repo)
            .map(Into::into)
            .map_err(|e| match e {
                crate::error::GitHubError::Api { status: 404, .. } => {
                    TrackerError::ProjectNotFound(id.to_string())
                }
                other => TrackerError::from(other),
            })
    }

    fn create_project(&self, _project: &CreateProject) -> Result<Project> {
        Err(TrackerError::InvalidInput(
            "Creating repositories via this tool is not supported. Please use the GitHub web interface or gh CLI."
                .to_string(),
        ))
    }

    fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        // For GitHub, the project identifier is "owner/repo"
        // If already in that format, validate it; otherwise build it
        if identifier.contains('/') {
            let project = self.get_project(identifier)?;
            Ok(project.short_name)
        } else {
            // Assume it's a repo name under the configured owner
            let full = format!("{}/{}", self.owner(), identifier);
            let project = self.get_project(&full)?;
            Ok(project.short_name)
        }
    }

    fn get_project_custom_fields(&self, _project_id: &str) -> Result<Vec<ProjectCustomField>> {
        Ok(get_standard_custom_fields())
    }

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        Ok(self.list_labels()?.into_iter().map(Into::into).collect())
    }

    fn create_tag(&self, tag: &CreateTag) -> Result<IssueTag> {
        let color = tag
            .color
            .as_deref()
            .unwrap_or("#ededed")
            .trim_start_matches('#')
            .to_string();

        let create = crate::models::CreateGitHubLabel {
            name: tag.name.clone(),
            color,
            description: tag.description.clone(),
        };

        Ok(self.create_label(&create)?.into())
    }

    fn delete_tag(&self, name: &str) -> Result<()> {
        Ok(self.delete_label(name)?)
    }

    fn update_tag(&self, current_name: &str, tag: &CreateTag) -> Result<IssueTag> {
        let new_name = if tag.name != current_name {
            Some(tag.name.clone())
        } else {
            None
        };

        let color = tag
            .color
            .as_ref()
            .map(|c| c.trim_start_matches('#').to_string());

        let update = crate::models::UpdateGitHubLabel {
            new_name,
            color,
            description: tag.description.clone(),
        };

        Ok(self.update_label(current_name, &update)?.into())
    }

    fn get_issue_links(&self, _issue_id: &str) -> Result<Vec<IssueLink>> {
        // GitHub has no formal issue link system
        Ok(Vec::new())
    }

    fn link_issues(
        &self,
        _source: &str,
        _target: &str,
        _link_type: &str,
        _direction: &str,
    ) -> Result<()> {
        Err(TrackerError::InvalidInput(
            "GitHub does not support formal issue links. Reference issues via #number in comments instead."
                .to_string(),
        ))
    }

    fn link_subtask(&self, child: &str, parent: &str) -> Result<()> {
        let child_number = parse_issue_number(child)?;
        let parent_number = parse_issue_number(parent)?;

        // Fetch the child issue to get its global ID (the sub-issues API needs id, not number)
        let child_issue = self.get_issue(child_number)?;

        Ok(self.add_sub_issue(parent_number, child_issue.id)?)
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        let number = parse_issue_number(issue_id)?;
        Ok(self.add_comment(number, text)?.into())
    }

    fn add_issue_comment_attachment(
        &self,
        _issue_id: &str,
        _text: &str,
        _upload: &AttachmentUpload,
    ) -> Result<Comment> {
        Err(TrackerError::InvalidInput(
            "GitHub Issues does not expose a public REST API for uploading issue comment file attachments."
                .to_string(),
        ))
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        let number = parse_issue_number(issue_id)?;
        Ok(self
            .get_comments(number)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn get_comments_page(&self, issue_id: &str, limit: usize, skip: usize) -> Result<Vec<Comment>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let number = parse_issue_number(issue_id)?;
        let per_page = 100;
        let mut page = (skip / per_page) + 1;
        let mut page_offset = skip % per_page;
        let mut comments = Vec::new();

        while comments.len() < limit {
            let page_comments = self.get_comments_page(number, per_page, page)?;
            let page_len = page_comments.len();
            let remaining = limit - comments.len();

            comments.extend(
                page_comments
                    .into_iter()
                    .skip(page_offset)
                    .take(remaining)
                    .map(Into::into),
            );

            if page_len < per_page {
                break;
            }

            page += 1;
            page_offset = 0;
        }

        Ok(comments)
    }
}

// ============================================================================
// KnowledgeBase Implementation
// ============================================================================

use std::collections::HashSet;

/// Generate a URL-safe slug from text
fn slugify(text: &str) -> String {
    let slug: String = text
        .chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '/' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // Collapse consecutive hyphens and trim
    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    result.trim_matches('-').to_string()
}

/// Convert a WikiPage to an Article
fn wiki_page_to_article(page: WikiPage, owner: &str, repo: &str) -> Article {
    Article {
        id: page.slug.clone(),
        id_readable: format!("{}/{}#{}", owner, repo, page.slug),
        summary: page.title,
        content: Some(page.content),
        project: ProjectRef {
            id: format!("{}/{}", owner, repo),
            name: Some(repo.to_string()),
            short_name: Some(format!("{}/{}", owner, repo)),
        },
        parent_article: page.parent.map(|p| ArticleRef {
            id: p.clone(),
            id_readable: Some(format!("{}/{}#{}", owner, repo, p)),
            summary: Some(p.replace('-', " ")),
        }),
        has_children: false, // Will be set by caller if needed
        tags: page
            .tags
            .into_iter()
            .map(|t| Tag {
                id: t.clone(),
                name: t,
            })
            .collect(),
        created: page.created,
        updated: page.updated,
        reporter: page.author.map(|name| CommentAuthor {
            login: name.clone(),
            name: Some(name),
        }),
    }
}

impl KnowledgeBase for GitHubClient {
    fn get_article(&self, id: &str) -> Result<Article> {
        let wiki = self.wiki();
        let page = wiki.get_page(id)?;
        let mut article = wiki_page_to_article(page, self.owner(), self.repo());

        // Check if this page has children
        if let Ok(children) = wiki.get_child_pages(id) {
            article.has_children = !children.is_empty();
        }

        Ok(article)
    }

    fn list_articles(
        &self,
        project_id: Option<&str>,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<Article>> {
        // Filter by project if specified
        if let Some(proj) = project_id {
            let expected_project = format!("{}/{}", self.owner(), self.repo());
            if proj != expected_project {
                return Ok(Vec::new()); // Project doesn't match
            }
        }

        let wiki = self.wiki();
        let pages = wiki.list_pages()?;

        // Single-pass parent set for has_children
        let parent_slugs: HashSet<String> = pages.iter().filter_map(|p| p.parent.clone()).collect();

        let articles: Vec<Article> = pages
            .into_iter()
            .skip(skip)
            .take(limit)
            .map(|page| {
                let mut article = wiki_page_to_article(page, self.owner(), self.repo());
                article.has_children = parent_slugs.contains(&article.id);
                article
            })
            .collect();

        Ok(articles)
    }

    fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>> {
        let wiki = self.wiki();
        let pages = wiki.search_pages(query)?;

        // Single-pass parent set for has_children
        let parent_slugs: HashSet<String> = pages.iter().filter_map(|p| p.parent.clone()).collect();

        let articles: Vec<Article> = pages
            .into_iter()
            .skip(skip)
            .take(limit)
            .map(|page| {
                let mut article = wiki_page_to_article(page, self.owner(), self.repo());
                article.has_children = parent_slugs.contains(&article.id);
                article
            })
            .collect();

        Ok(articles)
    }

    fn create_article(&self, article: &CreateArticle) -> Result<Article> {
        // Only accept the canonical owner/repo format
        let expected_project = format!("{}/{}", self.owner(), self.repo());
        if article.project_id != expected_project {
            return Err(TrackerError::InvalidInput(format!(
                "Project '{}' does not match current repository '{}'",
                article.project_id, expected_project
            )));
        }

        let wiki = self.wiki();

        // Generate slug from title
        let slug = if let Some(parent) = &article.parent_article_id {
            format!("{}/{}", parent, slugify(&article.summary))
        } else {
            slugify(&article.summary)
        };

        let content = article.content.as_deref().unwrap_or("");

        let page = wiki.create_page(&slug, &article.summary, content, article.tags.clone())?;

        Ok(wiki_page_to_article(page, self.owner(), self.repo()))
    }

    fn update_article(&self, id: &str, update: &UpdateArticle) -> Result<Article> {
        let wiki = self.wiki();

        let page = wiki.update_page(
            id,
            update.summary.as_deref(),
            update.content.as_deref(),
            if update.tags.is_empty() {
                None
            } else {
                Some(update.tags.clone())
            },
        )?;

        Ok(wiki_page_to_article(page, self.owner(), self.repo()))
    }

    fn delete_article(&self, id: &str) -> Result<()> {
        let wiki = self.wiki();

        Ok(wiki.delete_page(id)?)
    }

    fn get_child_articles(&self, parent_id: &str) -> Result<Vec<Article>> {
        let wiki = self.wiki();

        let pages = wiki.get_child_pages(parent_id)?;

        let articles = pages
            .into_iter()
            .map(|page| wiki_page_to_article(page, self.owner(), self.repo()))
            .collect();

        Ok(articles)
    }

    fn move_article(&self, article_id: &str, new_parent_id: Option<&str>) -> Result<Article> {
        let wiki = self.wiki();

        let page = wiki.move_page(article_id, new_parent_id)?;

        Ok(wiki_page_to_article(page, self.owner(), self.repo()))
    }

    fn list_article_attachments(&self, article_id: &str) -> Result<Vec<ArticleAttachment>> {
        let wiki = self.wiki();
        let attachments = wiki.list_attachments(article_id)?;
        Ok(attachments
            .into_iter()
            .map(|attachment| ArticleAttachment {
                id: attachment.path,
                name: attachment.name,
                size: attachment.size,
                mime_type: attachment.mime_type,
                url: attachment.url,
                created: None,
            })
            .collect())
    }

    fn add_article_attachment(
        &self,
        article_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<ArticleAttachment>> {
        let wiki = self.wiki();
        let attachments = wiki.add_attachments(article_id, upload)?;
        Ok(attachments
            .into_iter()
            .map(|attachment| ArticleAttachment {
                id: attachment.path,
                name: attachment.name,
                size: attachment.size,
                mime_type: attachment.mime_type,
                url: attachment.url,
                created: None,
            })
            .collect())
    }

    fn get_article_comments(&self, _article_id: &str) -> Result<Vec<Comment>> {
        // GitHub wikis don't support comments
        Ok(Vec::new())
    }

    fn add_article_comment(&self, _article_id: &str, _text: &str) -> Result<Comment> {
        // GitHub wikis don't support comments
        Err(TrackerError::InvalidInput(
            "GitHub wikis do not support comments".to_string(),
        ))
    }
}
