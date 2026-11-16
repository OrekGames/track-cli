//! Implementation of tracker-core traits for GitHubClient

use tracker_core::pagination::{Page, fetch_window};
use tracker_core::{
    Article, ArticleAttachment, ArticleRef, AttachmentUpload, Comment, CommentAuthor,
    CreateArticle, CreateIssue, CreateProject, CreateTag, Issue, IssueAttachment,
    IssueHistoryEvent, IssueLink, IssueTag, IssueTracker, KnowledgeBase, Project,
    ProjectCustomField, ProjectRef, Result, SearchResult, Tag, TrackerError, UpdateArticle,
    UpdateIssue,
};

use crate::client::GitHubClient;
use crate::convert::{
    convert_query_to_github, create_issue_from_core, get_standard_custom_fields,
    github_issue_to_core, github_timeline_to_events, update_issue_from_core,
};
use crate::wiki::{WikiAttachment, WikiPage};

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
        // The in-page offset counts issues left after pull requests are dropped
        // and applies to the first page only, so it is applied here rather than
        // passed to `fetch_window` as its skip.
        let mut page_offset = skip % per_page;
        let mut total = None;

        let issues = fetch_window(limit, 0, |_| {
            let result = self.search_issues(&github_query, per_page, page)?;
            page += 1;
            if total.is_none() {
                total = Some(result.total_count);
            }
            // The raw page length, pull requests included, decides the last page.
            let last = result.items.len() < per_page;
            let issues = result
                .items
                .into_iter()
                .filter(|i| !i.is_pull_request())
                .skip(page_offset)
                .collect();
            page_offset = 0;
            Ok(Page {
                items: issues,
                last,
            })
        })?;
        let items: Vec<Issue> = issues
            .into_iter()
            .map(|i| github_issue_to_core(i, &owner, &repo))
            .collect();

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
        Err(TrackerError::Unsupported(
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
                TrackerError::Api { status: 404, .. } => {
                    TrackerError::ProjectNotFound(id.to_string())
                }
                other => other,
            })
    }

    fn create_project(&self, _project: &CreateProject) -> Result<Project> {
        Err(TrackerError::Unsupported(
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
        self.delete_label(name)
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
        Err(TrackerError::Unsupported(
            "GitHub does not support formal issue links. Reference issues via #number in comments instead."
                .to_string(),
        ))
    }

    fn link_subtask(&self, child: &str, parent: &str) -> Result<()> {
        let child_number = parse_issue_number(child)?;
        let parent_number = parse_issue_number(parent)?;

        // Fetch the child issue to get its global ID (the sub-issues API needs id, not number)
        let child_issue = self.get_issue(child_number)?;

        self.add_sub_issue(parent_number, child_issue.id)
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
        // Start on the native page that holds comment `skip`.
        let mut page = (skip / per_page) + 1;
        let comments = fetch_window(limit, skip % per_page, |_| {
            let batch = self.get_comments_page(number, per_page, page)?;
            page += 1;
            let last = batch.len() < per_page;
            Ok(Page { items: batch, last })
        })?;
        Ok(comments.into_iter().map(Into::into).collect())
    }

    fn get_issue_history(&self, issue_id: &str) -> Result<Vec<IssueHistoryEvent>> {
        let number = parse_issue_number(issue_id)?;
        let timeline = self.get_issue_timeline(number)?;
        Ok(github_timeline_to_events(timeline))
    }

    fn web_url(&self, issue_id: Option<&str>) -> Result<Option<String>> {
        let Some(id) = issue_id else {
            return Ok(Some(self.issues_web_url(None, None)));
        };
        let number = parse_issue_number(id)?;
        // "owner/repo#N" names its own repository; "N" and "#N" mean this one.
        let repo_path = id
            .rsplit_once('#')
            .map(|(repo_path, _)| repo_path)
            .filter(|repo_path| repo_path.contains('/'));
        Ok(Some(self.issues_web_url(repo_path, Some(number))))
    }

    // GitHub issues are only ever open or closed.
    fn default_start_state(&self) -> &'static str {
        "open"
    }

    fn default_complete_state(&self) -> &'static str {
        "closed"
    }
}

// ============================================================================
// KnowledgeBase Implementation
// ============================================================================

use std::collections::HashSet;

/// Generate a URL-safe slug from text.
///
/// Non-ASCII characters are transliterated to ASCII via `deunicode` so that
/// non-English titles produce meaningful, distinct slugs:
/// - `"Café au lait"` → `"cafe-au-lait"`
/// - `"はじめに"` → `"hajimeni"`
/// - `"Открыто"` → `"otkryto"`
///
/// Pure ASCII titles produce identical slugs to a no-transliteration pass, so
/// existing wikis don't churn.
fn slugify(text: &str) -> String {
    let ascii = deunicode::deunicode(text);
    let slug: String = ascii
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

fn wiki_pages_to_articles(
    pages: Vec<WikiPage>,
    owner: &str,
    repo: &str,
    limit: usize,
    skip: usize,
) -> Vec<Article> {
    let parent_slugs: HashSet<String> = pages.iter().filter_map(|p| p.parent.clone()).collect();

    pages
        .into_iter()
        .skip(skip)
        .take(limit)
        .map(|page| {
            let mut article = wiki_page_to_article(page, owner, repo);
            article.has_children = parent_slugs.contains(&article.id);
            article
        })
        .collect()
}

fn wiki_attachment_to_article_attachment(attachment: WikiAttachment) -> ArticleAttachment {
    ArticleAttachment {
        id: attachment.path,
        name: attachment.name,
        size: attachment.size,
        mime_type: attachment.mime_type,
        url: attachment.url,
        created: None,
    }
}

impl GitHubClient {
    /// Whether `project_id` names this client's repository. GitHub owner and
    /// repository names are case-insensitive, and `resolve_project_id` returns
    /// GitHub's canonical casing, which need not match the configured one.
    fn is_configured_repo(&self, project_id: &str) -> bool {
        project_id.eq_ignore_ascii_case(&format!("{}/{}", self.owner(), self.repo()))
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
        // The wiki only holds this repository's pages.
        if project_id.is_some_and(|proj| !self.is_configured_repo(proj)) {
            return Ok(Vec::new());
        }

        let wiki = self.wiki();
        let pages = wiki.list_pages()?;

        Ok(wiki_pages_to_articles(
            pages,
            self.owner(),
            self.repo(),
            limit,
            skip,
        ))
    }

    fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>> {
        let wiki = self.wiki();
        let pages = wiki.search_pages(query)?;

        Ok(wiki_pages_to_articles(
            pages,
            self.owner(),
            self.repo(),
            limit,
            skip,
        ))
    }

    // The wiki is read whole, so return everything in one call rather than
    // letting the default offset paging re-read it for every page.
    fn list_all_articles(
        &self,
        project_id: Option<&str>,
        max_results: usize,
    ) -> Result<Vec<Article>> {
        if max_results == 0 {
            return Ok(Vec::new());
        }
        self.list_articles(project_id, max_results, 0)
    }

    fn search_all_articles(&self, query: &str, max_results: usize) -> Result<Vec<Article>> {
        if max_results == 0 {
            return Ok(Vec::new());
        }
        self.search_articles(query, max_results, 0)
    }

    fn create_article(&self, article: &CreateArticle) -> Result<Article> {
        if !self.is_configured_repo(&article.project_id) {
            return Err(TrackerError::InvalidInput(format!(
                "Project '{}' does not match current repository '{}/{}'",
                article.project_id,
                self.owner(),
                self.repo()
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

        wiki.delete_page(id)
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
        let attachments = self.wiki().list_attachments(article_id)?;
        Ok(attachments
            .into_iter()
            .map(wiki_attachment_to_article_attachment)
            .collect())
    }

    fn add_article_attachment(
        &self,
        article_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<ArticleAttachment>> {
        let attachments = self.wiki().add_attachments(article_id, upload)?;
        Ok(attachments
            .into_iter()
            .map(wiki_attachment_to_article_attachment)
            .collect())
    }

    fn get_article_comments(&self, _article_id: &str) -> Result<Vec<Comment>> {
        // GitHub wikis don't support comments
        Ok(Vec::new())
    }

    fn add_article_comment(&self, _article_id: &str, _text: &str) -> Result<Comment> {
        // GitHub wikis don't support comments
        Err(TrackerError::Unsupported(
            "GitHub wikis do not support comments".to_string(),
        ))
    }
}

#[cfg(test)]
mod slugify_tests {
    use super::{slugify, wiki_pages_to_articles};
    use crate::client::GitHubClient;
    use crate::wiki::WikiPage;

    fn wiki_page(slug: &str, title: &str, content: &str, parent: Option<&str>) -> WikiPage {
        WikiPage {
            slug: slug.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            parent: parent.map(str::to_string),
            tags: Vec::new(),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            author: None,
        }
    }

    #[test]
    fn ascii_titles_unchanged() {
        assert_eq!(slugify("Getting Started"), "getting-started");
        assert_eq!(slugify("API Reference"), "api-reference");
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn collapses_consecutive_separators() {
        assert_eq!(slugify("Hello  -  World"), "hello-world");
        assert_eq!(slugify("--Hello--"), "hello");
    }

    #[test]
    fn preserves_path_separator() {
        assert_eq!(slugify("Parent/Child"), "parent/child");
    }

    #[test]
    fn accented_latin_transliterates() {
        assert_eq!(slugify("Café au lait"), "cafe-au-lait");
        assert_eq!(slugify("Crème brûlée"), "creme-brulee");
        assert_eq!(slugify("Geöffnet"), "geoffnet");
    }

    #[test]
    fn cyrillic_transliterates() {
        assert_eq!(slugify("Открыто"), "otkryto");
        assert_eq!(slugify("В работе"), "v-rabote");
    }

    #[test]
    fn cjk_transliterates_to_romaji_or_pinyin() {
        // deunicode's CJK output is approximate but produces stable,
        // distinct, ASCII strings — which is what slugs need.
        let hajimeni = slugify("はじめに");
        assert!(!hajimeni.is_empty(), "CJK slug should not be empty");
        assert!(
            hajimeni.is_ascii(),
            "slug should be all ASCII: {hajimeni:?}"
        );

        let kanryo = slugify("完了");
        assert!(!kanryo.is_empty(), "CJK slug should not be empty");

        // Distinct titles produce distinct slugs.
        assert_ne!(hajimeni, kanryo);
    }

    #[test]
    fn distinct_non_ascii_titles_no_longer_collapse() {
        // Before deunicode: both became "----" / similar.
        let a = slugify("はじめに");
        let b = slugify("使い方");
        let c = slugify("完了");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn wiki_pages_to_articles_sets_children_and_truncates() {
        let pages = vec![
            wiki_page("Parent", "Parent", "root", None),
            wiki_page("Parent/Child", "Child", "nested", Some("Parent")),
            wiki_page("Other", "Other", "root", None),
        ];

        let articles = wiki_pages_to_articles(pages, "owner", "repo", 2, 0);

        assert_eq!(articles.len(), 2);
        assert_eq!(articles[0].id, "Parent");
        assert!(articles[0].has_children);
        assert_eq!(articles[1].id, "Parent/Child");
        assert!(!articles[1].has_children);
    }

    #[test]
    fn wiki_page_matches_query_searches_title_content_and_tags() {
        let mut page = wiki_page("Guide", "Install Guide", "setup notes", None);
        page.tags = vec!["docs".to_string()];

        assert!(page.matches_query("install"));
        assert!(page.matches_query("setup"));
        assert!(page.matches_query("docs"));
        assert!(!page.matches_query("missing"));
    }

    #[test]
    fn configured_repo_matches_canonical_casing() {
        // `resolve_project_id` returns GitHub's canonical `full_name`, which can
        // differ in case from the owner/repo typed into the config.
        let client = GitHubClient::new("orekgames", "track-cli", "token");

        assert!(client.is_configured_repo("OrekGames/track-cli"));
        assert!(client.is_configured_repo("orekgames/Track-CLI"));
        assert!(!client.is_configured_repo("orekgames/other-repo"));
        assert!(!client.is_configured_repo("track-cli"));
    }
}
