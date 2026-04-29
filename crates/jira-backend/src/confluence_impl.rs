//! Implementation of tracker-core KnowledgeBase trait for ConfluenceClient

use chrono::{DateTime, Utc};
use tracker_core::{
    Article, ArticleAttachment, ArticleRef, AttachmentUpload, Comment, CommentAuthor,
    CreateArticle, KnowledgeBase, ProjectRef, Result, TrackerError, UpdateArticle,
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
        skip: usize,
    ) -> Result<Vec<Article>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut articles = Vec::new();
        let mut remaining_skip = skip;
        let mut cursor: Option<String> = None;

        loop {
            let page = self
                .list_pages(project_id, limit, cursor.as_deref())
                .map_err(TrackerError::from)?;
            let page_len = page.results.len();

            if remaining_skip >= page_len {
                remaining_skip -= page_len;
            } else {
                let remaining = limit - articles.len();
                articles.extend(
                    page.results
                        .into_iter()
                        .skip(remaining_skip)
                        .take(remaining)
                        .map(confluence_page_to_article),
                );
                remaining_skip = 0;
            }

            if articles.len() >= limit || page_len == 0 {
                break;
            }

            cursor = page.links.as_ref().and_then(extract_next_cursor);
            if cursor.is_none() {
                break;
            }
        }

        Ok(articles)
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

    fn add_article_attachment(
        &self,
        article_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<ArticleAttachment>> {
        self.add_content_attachments(article_id, upload)
            .map(|attachments| {
                attachments
                    .into_iter()
                    .map(confluence_uploaded_attachment_to_article_attachment)
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

    fn add_article_comment_attachment(
        &self,
        article_id: &str,
        text: &str,
        upload: &AttachmentUpload,
    ) -> Result<Comment> {
        let comment = self
            .add_page_comment(article_id, text)
            .map_err(TrackerError::from)?;
        self.add_content_attachments(&comment.id, upload)
            .map_err(TrackerError::from)?;
        Ok(confluence_comment_to_comment(comment))
    }

    fn supports_article_comment_attachments(&self) -> bool {
        true
    }
}

// ============================================================================
// Conversion Functions
// ============================================================================

fn extract_next_cursor(links: &ConfluencePaginationLinks) -> Option<String> {
    let next = links.next.as_ref()?;
    let query = next
        .split_once('?')
        .map_or(next.as_str(), |(_, query)| query);

    query.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        if key == "cursor" {
            urlencoding::decode(value).ok().map(|v| v.into_owned())
        } else {
            None
        }
    })
}

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

fn confluence_uploaded_attachment_to_article_attachment(
    att: ConfluenceAttachmentUpload,
) -> ArticleAttachment {
    let created = att
        .version
        .as_ref()
        .and_then(|v| v.when.as_ref())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));

    let (mime_type, size) = att
        .metadata
        .map(|metadata| {
            (
                metadata.media_type,
                metadata
                    .extensions
                    .and_then(|extensions| extensions.file_size)
                    .unwrap_or(0),
            )
        })
        .unwrap_or((None, 0));

    ArticleAttachment {
        id: att.id,
        name: att.title,
        size,
        mime_type,
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

/// Convert Markdown to Confluence storage format.
fn markdown_to_storage(markdown: &str) -> String {
    use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

    if looks_like_confluence_storage(markdown) {
        return markdown.to_string();
    }

    let parser = Parser::new_ext(
        markdown,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
    );

    let mut result = String::new();
    let mut code_block: Option<(String, String)> = None;
    let mut image: Option<(String, String)> = None;
    let mut in_table_head = false;

    for event in parser {
        if let Some((_, body)) = code_block.as_mut() {
            match event {
                Event::End(TagEnd::CodeBlock) => {
                    let (language, body) = code_block.take().expect("code block is open");
                    result.push_str(&storage_code_macro(&language, &body));
                }
                Event::Text(text)
                | Event::Code(text)
                | Event::Html(text)
                | Event::InlineHtml(text)
                | Event::InlineMath(text)
                | Event::DisplayMath(text) => body.push_str(&text),
                Event::SoftBreak | Event::HardBreak => body.push('\n'),
                _ => {}
            }
            continue;
        }

        if let Some((_, alt_text)) = image.as_mut() {
            match event {
                Event::End(TagEnd::Image) => {
                    let (url, alt_text) = image.take().expect("image is open");
                    result.push_str(&format!(
                        "<ac:image ac:alt=\"{}\"><ri:url ri:value=\"{}\" /></ac:image>",
                        html_escape(&alt_text),
                        html_escape(&url)
                    ));
                }
                Event::Text(text) | Event::Code(text) => alt_text.push_str(&text),
                Event::SoftBreak | Event::HardBreak => alt_text.push(' '),
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => result.push_str("<p>"),
                Tag::Heading { level, .. } => {
                    result.push_str(&format!("<{}>", heading_tag(level)));
                }
                Tag::BlockQuote(_) => result.push_str("<blockquote>"),
                Tag::CodeBlock(kind) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    code_block = Some((language, String::new()));
                }
                Tag::List(Some(start)) => {
                    if start == 1 {
                        result.push_str("<ol>");
                    } else {
                        result.push_str(&format!("<ol start=\"{start}\">"));
                    }
                }
                Tag::List(None) => result.push_str("<ul>"),
                Tag::Item => result.push_str("<li>"),
                Tag::Emphasis => result.push_str("<em>"),
                Tag::Strong => result.push_str("<strong>"),
                Tag::Strikethrough => result.push_str("<del>"),
                Tag::Link { dest_url, .. } => {
                    result.push_str(&format!("<a href=\"{}\">", html_escape(&dest_url)));
                }
                Tag::Image { dest_url, .. } => {
                    image = Some((dest_url.to_string(), String::new()));
                }
                Tag::Table(_) => result.push_str("<table><tbody>"),
                Tag::TableHead => in_table_head = true,
                Tag::TableRow => result.push_str("<tr>"),
                Tag::TableCell => {
                    result.push_str(if in_table_head { "<th>" } else { "<td>" });
                }
                Tag::DefinitionList => result.push_str("<dl>"),
                Tag::DefinitionListTitle => result.push_str("<dt>"),
                Tag::DefinitionListDefinition => result.push_str("<dd>"),
                Tag::HtmlBlock | Tag::FootnoteDefinition(_) | Tag::MetadataBlock(_) => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => result.push_str("</p>"),
                TagEnd::Heading(level) => {
                    result.push_str(&format!("</{}>", heading_tag(level)));
                }
                TagEnd::BlockQuote(_) => result.push_str("</blockquote>"),
                TagEnd::CodeBlock => {}
                TagEnd::List(true) => result.push_str("</ol>"),
                TagEnd::List(false) => result.push_str("</ul>"),
                TagEnd::Item => result.push_str("</li>"),
                TagEnd::Emphasis => result.push_str("</em>"),
                TagEnd::Strong => result.push_str("</strong>"),
                TagEnd::Strikethrough => result.push_str("</del>"),
                TagEnd::Link => result.push_str("</a>"),
                TagEnd::Image => {}
                TagEnd::Table => result.push_str("</tbody></table>"),
                TagEnd::TableHead => in_table_head = false,
                TagEnd::TableRow => result.push_str("</tr>"),
                TagEnd::TableCell => {
                    result.push_str(if in_table_head { "</th>" } else { "</td>" });
                }
                TagEnd::DefinitionList => result.push_str("</dl>"),
                TagEnd::DefinitionListTitle => result.push_str("</dt>"),
                TagEnd::DefinitionListDefinition => result.push_str("</dd>"),
                TagEnd::HtmlBlock | TagEnd::FootnoteDefinition | TagEnd::MetadataBlock(_) => {}
            },
            Event::Text(text) => result.push_str(&html_escape(&text)),
            Event::Code(text) => {
                result.push_str(&format!("<code>{}</code>", html_escape(&text)));
            }
            Event::InlineMath(text) => {
                result.push_str(&format!("<code>{}</code>", html_escape(&text)));
            }
            Event::DisplayMath(text) => {
                result.push_str("<p><code>");
                result.push_str(&html_escape(&text));
                result.push_str("</code></p>");
            }
            Event::Html(html) | Event::InlineHtml(html) => result.push_str(&html),
            Event::FootnoteReference(reference) => {
                result.push_str(&format!("<sup>{}</sup>", html_escape(&reference)));
            }
            Event::SoftBreak => result.push('\n'),
            Event::HardBreak => result.push_str("<br/>"),
            Event::Rule => result.push_str("<hr/>"),
            Event::TaskListMarker(checked) => {
                result.push_str(if checked { "[x] " } else { "[ ] " });
            }
        }
    }

    result
}

fn looks_like_confluence_storage(input: &str) -> bool {
    let trimmed = input.trim_start();
    let lower = trimmed.to_ascii_lowercase();

    [
        "<ac:",
        "<ri:",
        "<table",
        "<tbody",
        "<thead",
        "<tr",
        "<td",
        "<th",
        "<p",
        "<h1",
        "<h2",
        "<h3",
        "<h4",
        "<h5",
        "<h6",
        "<ul",
        "<ol",
        "<blockquote",
        "<pre",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn heading_tag(level: pulldown_cmark::HeadingLevel) -> &'static str {
    match level {
        pulldown_cmark::HeadingLevel::H1 => "h1",
        pulldown_cmark::HeadingLevel::H2 => "h2",
        pulldown_cmark::HeadingLevel::H3 => "h3",
        pulldown_cmark::HeadingLevel::H4 => "h4",
        pulldown_cmark::HeadingLevel::H5 => "h5",
        pulldown_cmark::HeadingLevel::H6 => "h6",
    }
}

fn storage_code_macro(language: &str, body: &str) -> String {
    let mut result = String::from("<ac:structured-macro ac:name=\"code\">");
    let language = language.trim();

    if !language.is_empty() {
        result.push_str(&format!(
            "<ac:parameter ac:name=\"language\">{}</ac:parameter>",
            html_escape(language)
        ));
    }

    result.push_str("<ac:plain-text-body><![CDATA[");
    result.push_str(&escape_cdata(body));
    result.push_str("]]></ac:plain-text-body></ac:structured-macro>");
    result
}

fn escape_cdata(input: &str) -> String {
    input.replace("]]>", "]]]]><![CDATA[>")
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracker_core::KnowledgeBase;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn page(id: &str, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "title": title,
            "spaceId": "SPACE"
        })
    }

    #[test]
    fn markdown_to_storage_renders_rich_markdown() {
        let markdown = r#"# Title

| A | B |
|---|---|
| 1<br>2 | **bold** |

```bash
echo hi
```

> **Note:** something
"#;

        let storage = markdown_to_storage(markdown);

        assert!(storage.contains("<h1>Title</h1>"));
        assert!(storage.contains("<table><tbody>"));
        assert!(storage.contains("<th>A</th>"));
        assert!(storage.contains("<td>1<br>2</td>"));
        assert!(storage.contains("<td><strong>bold</strong></td>"));
        assert!(storage.contains("<ac:structured-macro ac:name=\"code\">"));
        assert!(storage.contains("<ac:parameter ac:name=\"language\">bash</ac:parameter>"));
        assert!(storage.contains("<ac:plain-text-body><![CDATA[echo hi\n]]>"));
        assert!(
            storage.contains("<blockquote><p><strong>Note:</strong> something</p></blockquote>")
        );
        assert!(
            !storage.contains("| A | B |"),
            "table Markdown should be converted, got: {storage}"
        );
        assert!(
            !storage.contains("```"),
            "fenced code markers should not be emitted, got: {storage}"
        );
    }

    #[test]
    fn markdown_to_storage_preserves_native_storage_xml() {
        let native_storage = r#"<table><tbody><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></tbody></table>
<ac:structured-macro ac:name="info"><ac:rich-text-body><p>Hi</p></ac:rich-text-body></ac:structured-macro>"#;

        let storage = markdown_to_storage(native_storage);

        assert_eq!(storage, native_storage);
        assert!(!storage.contains("&lt;table&gt;"));
        assert!(!storage.contains("&lt;ac:structured-macro"));
    }

    #[test]
    fn markdown_to_storage_converts_markdown_that_mentions_storage_tags() {
        let storage = markdown_to_storage("# Title\n\n`<ac:structured-macro>`");

        assert!(storage.contains("<h1>Title</h1>"));
        assert!(storage.contains("<code>&lt;ac:structured-macro&gt;</code>"));
    }

    #[tokio::test]
    async fn list_articles_emulates_offset_with_cursor_pages() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages"))
            .and(query_param("limit", "2"))
            .and(query_param_is_missing("cursor"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    page("1", "First"),
                    page("2", "Second")
                ],
                "_links": {
                    "next": "/wiki/api/v2/pages?cursor=next-page"
                }
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages"))
            .and(query_param("limit", "2"))
            .and(query_param("cursor", "next-page"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    page("3", "Third"),
                    page("4", "Fourth")
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let articles = client.list_articles(None, 2, 2).unwrap();

        assert_eq!(articles.len(), 2);
        assert_eq!(articles[0].id, "3");
        assert_eq!(articles[1].id, "4");
    }
}
