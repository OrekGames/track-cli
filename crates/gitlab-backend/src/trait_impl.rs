//! Implementation of tracker-core traits for GitLabClient

use tracker_core::{
    Article, ArticleAttachment, ArticleRef, AttachmentUpload, Comment, CreateArticle, CreateIssue,
    CreateProject, CreateTag, Issue, IssueLink, IssueLinkType, IssueTag, IssueTracker,
    KnowledgeBase, Project, ProjectCustomField, ProjectRef, Result, SearchResult, TrackerError,
    UpdateArticle, UpdateIssue, User,
};

use crate::client::GitLabClient;
use crate::convert::{
    convert_query_to_gitlab_params, get_gitlab_link_types, get_standard_custom_fields,
    gitlab_issue_to_core, gitlab_link_to_core,
};
use crate::models::{
    CreateGitLabIssue, CreateGitLabIssueLink, CreateGitLabLabel, CreateGitLabWikiPage,
    GitLabWikiAttachment, GitLabWikiPage, UpdateGitLabIssue, UpdateGitLabLabel,
    UpdateGitLabWikiPage,
};

const ATTACHMENT_BLOCK_START: &str = "<!-- track:attachments:start -->";
const ATTACHMENT_BLOCK_END: &str = "<!-- track:attachments:end -->";

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
        let project_id = self.project_id_str();

        if limit == 0 {
            return Ok(SearchResult::from_items(Vec::new()));
        }

        let per_page = 100;
        let mut page = (skip / per_page) + 1;
        let mut page_offset = skip % per_page;
        let mut issues = Vec::new();
        let mut total = None;

        while issues.len() < limit {
            // Use combined methods that read X-Total from the search response itself
            let (page_issues, page_total) = if search_text.is_empty() {
                self.list_issues_with_total(state.as_deref(), per_page, page)?
            } else {
                self.search_issues_with_total(
                    &search_text,
                    state.as_deref(),
                    labels.as_deref(),
                    per_page,
                    page,
                )?
            };

            if total.is_none() {
                total = page_total;
            }

            let page_len = page_issues.len();
            let remaining = limit - issues.len();
            issues.extend(page_issues.into_iter().skip(page_offset).take(remaining));

            if page_len < per_page {
                break;
            }

            page += 1;
            page_offset = 0;
        }

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
            // GitLab's work item hierarchy requires child items to be Task type
            issue_type: if issue.parent.is_some() {
                Some("task".to_string())
            } else {
                None
            },
        };

        let created = self.create_issue(&create)?;

        // If a parent was requested, set it via the GraphQL API after creation
        if let Some(ref parent_id) = issue.parent {
            let parent_iid = parse_issue_iid(parent_id)?;
            let parent_issue = self.get_issue(parent_iid)?;
            self.set_work_item_parent(created.id, parent_issue.id)?;
        }

        let project_id = self.project_id_str();
        Ok(gitlab_issue_to_core(created, &project_id))
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        let iid = parse_issue_iid(id)?;

        // Check for state changes in custom_fields
        let state_event = update.custom_fields.iter().find_map(|cf| match cf {
            tracker_core::CustomFieldUpdate::State { name, value }
                if name.to_lowercase() == "status"
                    || name.to_lowercase() == "state"
                    || name.to_lowercase() == "stage" =>
            {
                match value.to_lowercase().as_str() {
                    "closed" | "resolved" | "done" | "completed" => Some("close".to_string()),
                    "open" | "opened" | "reopen" | "reopened" | "in progress" | "develop" => {
                        Some("reopen".to_string())
                    }
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

        // Only call REST update if there are actual REST fields to update;
        // GitLab's PUT endpoint rejects empty bodies with 400
        let has_rest_fields = gitlab_update.title.is_some()
            || gitlab_update.description.is_some()
            || gitlab_update.labels.is_some()
            || gitlab_update.state_event.is_some()
            || gitlab_update.assignee_ids.is_some()
            || gitlab_update.milestone_id.is_some();

        let updated = if has_rest_fields {
            self.update_issue(iid, &gitlab_update)?
        } else {
            self.get_issue(iid)?
        };

        // If a parent was requested, set it via the GraphQL API
        if let Some(ref parent_id) = update.parent {
            let parent_iid = parse_issue_iid(parent_id)?;
            let parent_issue = self.get_issue(parent_iid)?;
            self.set_work_item_parent(updated.id, parent_issue.id)?;
        }

        let project_id = self.project_id_str();
        Ok(gitlab_issue_to_core(updated, &project_id))
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
        let gitlab_link_type = self.resolve_link_type(link_type);

        let project_id = self.project_id_str();
        let link = CreateGitLabIssueLink {
            target_project_id: project_id,
            target_issue_iid: target_iid,
            link_type: gitlab_link_type,
        };

        Ok(self.create_issue_link(source_iid, &link)?)
    }

    fn unlink_issues(&self, source: &str, link_id: &str) -> Result<()> {
        let iid = parse_issue_iid(source)?;
        let issue_link_id: u64 = link_id.parse().map_err(|_| {
            TrackerError::InvalidInput(format!(
                "Invalid GitLab link ID '{}': must be a number",
                link_id
            ))
        })?;
        Ok(self.delete_issue_link(iid, issue_link_id)?)
    }

    fn link_subtask(&self, child: &str, parent: &str) -> Result<()> {
        let child_iid = parse_issue_iid(child)?;
        let parent_iid = parse_issue_iid(parent)?;

        // Fetch both issues to get their global IDs (GraphQL needs global, not IID)
        let child_issue = self.get_issue(child_iid)?;
        let parent_issue = self.get_issue(parent_iid)?;

        // Use GraphQL workItemUpdate to set the parent
        Ok(self.set_work_item_parent(child_issue.id, parent_issue.id)?)
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        let iid = parse_issue_iid(issue_id)?;
        Ok(self.add_note(iid, text)?.into())
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        self.get_comments_page(issue_id, 100, 0)
    }

    fn get_comments_page(&self, issue_id: &str, limit: usize, skip: usize) -> Result<Vec<Comment>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let iid = parse_issue_iid(issue_id)?;
        let per_page = 100;
        let mut page = 1;
        let mut remaining_skip = skip;
        let mut comments = Vec::new();

        while comments.len() < limit {
            let raw_notes = self.get_notes_page_raw(iid, per_page, page)?;
            let page_len = raw_notes.len();
            let visible_notes: Vec<_> = raw_notes.into_iter().filter(|n| !n.system).collect();

            if remaining_skip >= visible_notes.len() {
                remaining_skip -= visible_notes.len();
            } else {
                let remaining = limit - comments.len();
                comments.extend(
                    visible_notes
                        .into_iter()
                        .skip(remaining_skip)
                        .take(remaining)
                        .map(Into::into),
                );
                remaining_skip = 0;
            }

            if page_len < per_page {
                break;
            }

            page += 1;
        }

        Ok(comments)
    }
}

// ==================== KnowledgeBase via project wikis ====================

impl KnowledgeBase for GitLabClient {
    fn get_article(&self, id: &str) -> Result<Article> {
        Ok(gitlab_wiki_page_to_article(
            self.get_wiki_page(id)?,
            &self.project_id_str(),
        ))
    }

    fn list_articles(
        &self,
        project_id: Option<&str>,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<Article>> {
        if let Some(project) = project_id
            && project != self.project_id_str()
        {
            return Ok(Vec::new());
        }

        Ok(self
            .list_wiki_pages(true)?
            .into_iter()
            .skip(skip)
            .take(limit)
            .map(|page| gitlab_wiki_page_to_article(page, &self.project_id_str()))
            .collect())
    }

    fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>> {
        let query = query.to_lowercase();
        Ok(self
            .list_wiki_pages(true)?
            .into_iter()
            .filter(|page| {
                page.title.to_lowercase().contains(&query)
                    || page
                        .content
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
            })
            .skip(skip)
            .take(limit)
            .map(|page| gitlab_wiki_page_to_article(page, &self.project_id_str()))
            .collect())
    }

    fn create_article(&self, article: &CreateArticle) -> Result<Article> {
        if article.project_id != self.project_id_str() {
            return Err(TrackerError::InvalidInput(format!(
                "Project '{}' does not match configured GitLab project '{}'",
                article.project_id,
                self.project_id_str()
            )));
        }

        let title = if let Some(parent) = &article.parent_article_id {
            format!("{}/{}", parent.trim_end_matches('/'), article.summary)
        } else {
            article.summary.clone()
        };
        let page = CreateGitLabWikiPage {
            title,
            content: article.content.clone().unwrap_or_default(),
            format: Some("markdown".to_string()),
        };

        Ok(gitlab_wiki_page_to_article(
            self.create_wiki_page(&page)?,
            &self.project_id_str(),
        ))
    }

    fn update_article(&self, id: &str, update: &UpdateArticle) -> Result<Article> {
        let page = UpdateGitLabWikiPage {
            title: update.summary.clone(),
            content: update.content.clone(),
            format: Some("markdown".to_string()),
        };

        Ok(gitlab_wiki_page_to_article(
            self.update_wiki_page(id, &page)?,
            &self.project_id_str(),
        ))
    }

    fn delete_article(&self, id: &str) -> Result<()> {
        Ok(self.delete_wiki_page(id)?)
    }

    fn get_child_articles(&self, parent_id: &str) -> Result<Vec<Article>> {
        let prefix = format!("{}/", parent_id.trim_end_matches('/'));
        Ok(self
            .list_wiki_pages(true)?
            .into_iter()
            .filter(|page| page.slug.starts_with(&prefix))
            .map(|page| gitlab_wiki_page_to_article(page, &self.project_id_str()))
            .collect())
    }

    fn move_article(&self, _article_id: &str, _new_parent_id: Option<&str>) -> Result<Article> {
        Err(TrackerError::InvalidInput(
            "Moving GitLab wiki pages is not supported by this backend".to_string(),
        ))
    }

    fn list_article_attachments(&self, article_id: &str) -> Result<Vec<ArticleAttachment>> {
        let page = self.get_wiki_page(article_id)?;
        Ok(
            parse_managed_attachment_block(page.content.as_deref().unwrap_or_default())
                .into_iter()
                .map(|(name, markdown, url)| ArticleAttachment {
                    id: markdown,
                    name,
                    size: 0,
                    mime_type: None,
                    url: Some(url),
                    created: None,
                })
                .collect(),
        )
    }

    fn add_article_attachment(
        &self,
        article_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<ArticleAttachment>> {
        if upload.comment.is_some() {
            return Err(TrackerError::InvalidInput(
                "GitLab wiki attachment upload does not support --comment".to_string(),
            ));
        }

        let page = self.get_wiki_page(article_id)?;
        let mut entries =
            parse_managed_attachment_block(page.content.as_deref().unwrap_or_default());
        let mut uploaded = Vec::new();

        for file in &upload.files {
            let attachment = self.upload_wiki_attachment(file)?;
            entries.retain(|(_, _, url)| url != &attachment.link.url);
            entries.push((
                attachment.file_name.clone(),
                attachment.link.markdown.clone(),
                attachment.link.url.clone(),
            ));
            uploaded.push(gitlab_wiki_attachment_to_article_attachment(attachment));
        }

        let content =
            replace_managed_attachment_block(page.content.as_deref().unwrap_or_default(), &entries);
        let update = UpdateGitLabWikiPage {
            title: None,
            content: Some(content),
            format: Some("markdown".to_string()),
        };
        self.update_wiki_page(article_id, &update)?;

        Ok(uploaded)
    }

    fn get_article_comments(&self, _article_id: &str) -> Result<Vec<Comment>> {
        Ok(Vec::new())
    }

    fn add_article_comment(&self, _article_id: &str, _text: &str) -> Result<Comment> {
        Err(TrackerError::InvalidInput(
            "GitLab wiki pages do not support comments".to_string(),
        ))
    }
}

fn gitlab_wiki_page_to_article(page: GitLabWikiPage, project_id: &str) -> Article {
    let parent_article = page.slug.rsplit_once('/').map(|(parent, _)| ArticleRef {
        id: parent.to_string(),
        id_readable: Some(parent.to_string()),
        summary: Some(parent.replace('-', " ")),
    });

    Article {
        id: page.slug.clone(),
        id_readable: page.slug,
        summary: page.title,
        content: page.content,
        project: ProjectRef {
            id: project_id.to_string(),
            name: Some(project_id.to_string()),
            short_name: Some(project_id.to_string()),
        },
        parent_article,
        has_children: false,
        tags: Vec::new(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        reporter: None,
    }
}

fn gitlab_wiki_attachment_to_article_attachment(
    attachment: GitLabWikiAttachment,
) -> ArticleAttachment {
    ArticleAttachment {
        id: attachment.file_path,
        name: attachment.file_name,
        size: 0,
        mime_type: None,
        url: Some(attachment.link.url),
        created: None,
    }
}

fn parse_managed_attachment_block(content: &str) -> Vec<(String, String, String)> {
    let mut attachments = Vec::new();
    let mut in_block = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == ATTACHMENT_BLOCK_START {
            in_block = true;
            continue;
        }
        if trimmed == ATTACHMENT_BLOCK_END {
            break;
        }
        if !in_block {
            continue;
        }

        let markdown = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some((name, url)) = parse_markdown_link(markdown) {
            attachments.push((name, markdown.to_string(), url));
        }
    }

    attachments
}

fn parse_markdown_link(markdown: &str) -> Option<(String, String)> {
    let rest = markdown
        .strip_prefix("![")
        .or_else(|| markdown.strip_prefix('['))?;
    let (name, rest) = rest.split_once("](")?;
    let url = rest.strip_suffix(')')?;
    Some((name.to_string(), url.to_string()))
}

fn replace_managed_attachment_block(
    content: &str,
    attachments: &[(String, String, String)],
) -> String {
    let mut output = Vec::new();
    let mut in_block = false;
    let mut replaced = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == ATTACHMENT_BLOCK_START {
            if !replaced {
                push_managed_attachment_block(&mut output, attachments);
                replaced = true;
            }
            in_block = true;
            continue;
        }
        if trimmed == ATTACHMENT_BLOCK_END {
            in_block = false;
            continue;
        }
        if !in_block {
            output.push(line.to_string());
        }
    }

    if !replaced {
        if !output.is_empty() && output.last().is_some_and(|line| !line.is_empty()) {
            output.push(String::new());
        }
        push_managed_attachment_block(&mut output, attachments);
    }

    output.join("\n")
}

fn push_managed_attachment_block(
    output: &mut Vec<String>,
    attachments: &[(String, String, String)],
) {
    output.push(ATTACHMENT_BLOCK_START.to_string());
    output.push("## Attachments".to_string());
    for (_, markdown, _) in attachments {
        output.push(format!("- {}", markdown));
    }
    output.push(ATTACHMENT_BLOCK_END.to_string());
}
