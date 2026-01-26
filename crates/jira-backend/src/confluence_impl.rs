//! Implementation of tracker-core KnowledgeBase trait for ConfluenceClient

use chrono::{DateTime, Utc};
use tracker_core::{
    Article, ArticleAttachment, ArticleRef, Comment, CommentAuthor, CreateArticle, KnowledgeBase,
    ProjectRef, Result, TrackerError, UpdateArticle,
};

use crate::confluence::ConfluenceClient;
use crate::models::confluence::*;

impl KnowledgeBase for ConfluenceClient {
    fn get_article(&self, id: &str) -> Result<Article> {
        self.get_page(id)
            .map(confluence_page_to_article)
            .map_err(TrackerError::from)
    }

    fn list_articles(
        &self,
        project_id: Option<&str>,
        limit: usize,
        _skip: usize,
    ) -> Result<Vec<Article>> {
        // Note: Confluence v2 API uses cursor-based pagination, not offset-based
        // The skip parameter is ignored here
        self.list_pages(project_id, limit, None)
            .map(|r| {
                r.results
                    .into_iter()
                    .map(confluence_page_to_article)
                    .collect()
            })
            .map_err(TrackerError::from)
    }

    fn search_articles(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Article>> {
        self.search_pages(query, limit, skip)
            .map(|r| {
                r.results
                    .into_iter()
                    .filter_map(|hit| hit.content.map(search_content_to_article))
                    .collect()
            })
            .map_err(TrackerError::from)
    }

    fn create_article(&self, article: &CreateArticle) -> Result<Article> {
        let page = CreateConfluencePage {
            space_id: article.project_id.clone(),
            title: article.summary.clone(),
            parent_id: article.parent_article_id.clone(),
            body: CreateConfluenceBody {
                representation: "storage".to_string(),
                value: article
                    .content
                    .clone()
                    .map(|c| markdown_to_storage(&c))
                    .unwrap_or_else(|| "<p></p>".to_string()),
            },
            status: Some("current".to_string()),
        };

        self.create_page(&page)
            .map(confluence_page_to_article)
            .map_err(TrackerError::from)
    }

    fn update_article(&self, id: &str, update: &UpdateArticle) -> Result<Article> {
        // First get the current page to get the version number and current status
        let current = self.get_page(id).map_err(TrackerError::from)?;
        let current_version = current.version.as_ref().map(|v| v.number).unwrap_or(1);
        let current_status = current
            .status
            .clone()
            .unwrap_or_else(|| "current".to_string());

        let update_request = UpdateConfluencePage {
            id: id.to_string(),
            title: update.summary.clone(),
            body: update.content.as_ref().map(|c| CreateConfluenceBody {
                representation: "storage".to_string(),
                value: markdown_to_storage(c),
            }),
            version: UpdateConfluenceVersion {
                number: current_version + 1,
                message: None,
            },
            status: Some(current_status),
        };

        self.update_page(id, &update_request)
            .map(confluence_page_to_article)
            .map_err(TrackerError::from)
    }

    fn delete_article(&self, id: &str) -> Result<()> {
        self.delete_page(id).map_err(TrackerError::from)
    }

    fn get_child_articles(&self, parent_id: &str) -> Result<Vec<Article>> {
        self.get_child_pages(parent_id, 100)
            .map(|r| {
                r.results
                    .into_iter()
                    .map(confluence_page_to_article)
                    .collect()
            })
            .map_err(TrackerError::from)
    }

    fn move_article(&self, article_id: &str, new_parent_id: Option<&str>) -> Result<Article> {
        // Get current page
        let current = self.get_page(article_id).map_err(TrackerError::from)?;
        let current_version = current.version.as_ref().map(|v| v.number).unwrap_or(1);

        // Update with new parent
        // Note: Confluence v2 API may not support changing parent directly via PUT
        // This might need to use the move endpoint if available
        let update_request = UpdateConfluencePage {
            id: article_id.to_string(),
            title: None,
            body: None,
            version: UpdateConfluenceVersion {
                number: current_version + 1,
                message: Some("Moved article".to_string()),
            },
            status: None,
        };

        // For now, just return the current article
        // A full implementation would need to use the proper move API
        if new_parent_id.is_some() {
            return Err(TrackerError::InvalidInput(
                "Moving articles to a new parent is not yet supported in Confluence".to_string(),
            ));
        }

        self.update_page(article_id, &update_request)
            .map(confluence_page_to_article)
            .map_err(TrackerError::from)
    }

    fn list_article_attachments(&self, article_id: &str) -> Result<Vec<ArticleAttachment>> {
        self.get_page_attachments(article_id, 100)
            .map(|r| {
                r.results
                    .into_iter()
                    .map(confluence_attachment_to_article_attachment)
                    .collect()
            })
            .map_err(TrackerError::from)
    }

    fn get_article_comments(&self, article_id: &str) -> Result<Vec<Comment>> {
        self.get_page_comments(article_id, 100)
            .map(|r| {
                r.results
                    .into_iter()
                    .map(confluence_comment_to_comment)
                    .collect()
            })
            .map_err(TrackerError::from)
    }

    fn add_article_comment(&self, article_id: &str, text: &str) -> Result<Comment> {
        self.add_page_comment(article_id, text)
            .map(confluence_comment_to_comment)
            .map_err(TrackerError::from)
    }
}

// ============================================================================
// Conversion Functions
// ============================================================================

fn confluence_page_to_article(page: ConfluencePage) -> Article {
    let content = page.body.as_ref().and_then(|b| {
        b.storage
            .as_ref()
            .and_then(|s| s.value.clone())
            .map(|v| storage_to_text(&v))
    });

    let created = page
        .created_at
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let updated = page
        .version
        .as_ref()
        .and_then(|v| v.created_at.as_ref())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(created);

    Article {
        id: page.id.clone(),
        id_readable: page.id.clone(), // Confluence uses numeric IDs
        summary: page.title,
        content,
        project: ProjectRef {
            id: page.space_id.clone().unwrap_or_default(),
            name: None,
            short_name: None,
        },
        parent_article: page.parent_id.map(|pid| ArticleRef {
            id: pid.clone(),
            id_readable: Some(pid),
            summary: None,
        }),
        has_children: false, // Would need additional API call to determine
        tags: Vec::new(),    // Confluence uses labels, would need to fetch separately
        created,
        updated,
        reporter: page.author_id.map(|id| CommentAuthor {
            login: id,
            name: None,
        }),
    }
}

fn search_content_to_article(content: ConfluenceSearchContent) -> Article {
    let space_id = content
        .space
        .as_ref()
        .and_then(|s| s.id.map(|id| id.to_string()))
        .unwrap_or_default();

    Article {
        id: content.id.clone(),
        id_readable: content.id.clone(),
        summary: content.title.unwrap_or_default(),
        content: None, // Search results don't include full content
        project: ProjectRef {
            id: space_id,
            name: content.space.as_ref().and_then(|s| s.name.clone()),
            short_name: content.space.as_ref().and_then(|s| s.key.clone()),
        },
        parent_article: None,
        has_children: false,
        tags: Vec::new(),
        created: Utc::now(),
        updated: Utc::now(),
        reporter: None,
    }
}

fn confluence_attachment_to_article_attachment(att: ConfluenceAttachment) -> ArticleAttachment {
    let created = att
        .created_at
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));

    ArticleAttachment {
        id: att.id,
        name: att.title,
        size: att.file_size.unwrap_or(0),
        mime_type: att.media_type,
        url: att.links.and_then(|l| l.download),
        created,
    }
}

fn confluence_comment_to_comment(comment: ConfluenceComment) -> Comment {
    let text = comment
        .body
        .as_ref()
        .and_then(|b| b.storage.as_ref())
        .and_then(|s| s.value.clone())
        .map(|v| storage_to_text(&v))
        .unwrap_or_default();

    let created = comment
        .created_at
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));

    let author = comment.version.and_then(|v| {
        v.author_id.map(|id| CommentAuthor {
            login: id,
            name: None,
        })
    });

    Comment {
        id: comment.id,
        text,
        author,
        created,
    }
}

// ============================================================================
// Content Format Conversion
// ============================================================================

/// Convert Confluence storage format (XHTML) to plain text
fn storage_to_text(storage: &str) -> String {
    // Simple HTML tag stripping - a full implementation would use an HTML parser
    let mut result = storage.to_string();

    // Replace common block elements with newlines
    result = result.replace("<br/>", "\n");
    result = result.replace("<br />", "\n");
    result = result.replace("<br>", "\n");
    result = result.replace("</p>", "\n");
    result = result.replace("</div>", "\n");
    result = result.replace("</li>", "\n");
    result = result.replace("</h1>", "\n\n");
    result = result.replace("</h2>", "\n\n");
    result = result.replace("</h3>", "\n\n");
    result = result.replace("</h4>", "\n");
    result = result.replace("</h5>", "\n");
    result = result.replace("</h6>", "\n");

    // Strip all remaining HTML tags
    let mut in_tag = false;
    let mut output = String::new();
    for c in result.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            output.push(c);
        }
    }

    // Decode common HTML entities
    output = output.replace("&nbsp;", " ");
    output = output.replace("&amp;", "&");
    output = output.replace("&lt;", "<");
    output = output.replace("&gt;", ">");
    output = output.replace("&quot;", "\"");

    // Clean up multiple newlines
    while output.contains("\n\n\n") {
        output = output.replace("\n\n\n", "\n\n");
    }

    output.trim().to_string()
}

/// Convert Markdown to Confluence storage format (basic)
fn markdown_to_storage(markdown: &str) -> String {
    let mut result = String::new();

    for line in markdown.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        // Headers
        if let Some(text) = trimmed.strip_prefix("### ") {
            result.push_str(&format!("<h3>{}</h3>", html_escape(text)));
        } else if let Some(text) = trimmed.strip_prefix("## ") {
            result.push_str(&format!("<h2>{}</h2>", html_escape(text)));
        } else if let Some(text) = trimmed.strip_prefix("# ") {
            result.push_str(&format!("<h1>{}</h1>", html_escape(text)));
        }
        // Code blocks (simplified)
        else if trimmed.starts_with("```") {
            // Skip code fence markers
        }
        // List items
        else if let Some(text) = trimmed.strip_prefix("- ") {
            result.push_str(&format!("<li>{}</li>", html_escape(text)));
        } else if let Some(text) = trimmed.strip_prefix("* ") {
            result.push_str(&format!("<li>{}</li>", html_escape(text)));
        }
        // Regular paragraphs
        else {
            result.push_str(&format!("<p>{}</p>", html_escape(trimmed)));
        }
    }

    result
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
