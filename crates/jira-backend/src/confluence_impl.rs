//! Implementation of tracker-core KnowledgeBase trait for ConfluenceClient

use chrono::Utc;
use tracker_core::{
    Article, ArticleAttachment, ArticleRef, AttachmentUpload, Comment, CommentAuthor,
    CreateArticle, KnowledgeBase, ProjectRef, Result, TrackerError, UpdateArticle,
};

use crate::confluence::ConfluenceClient;
use crate::convert::parse_jira_datetime;
use crate::markdown::storage::{markdown_to_storage, storage_to_text};
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

    fn list_all_articles(
        &self,
        project_id: Option<&str>,
        max_results: usize,
    ) -> Result<Vec<Article>> {
        if max_results == 0 {
            return Ok(Vec::new());
        }

        let mut articles = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let remaining = max_results - articles.len();
            let page_limit = remaining.min(100);
            let page = self
                .list_pages(project_id, page_limit, cursor.as_deref())
                .map_err(TrackerError::from)?;
            let page_len = page.results.len();

            articles.extend(
                page.results
                    .into_iter()
                    .take(remaining)
                    .map(confluence_page_to_article),
            );

            if articles.len() >= max_results || page_len == 0 {
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
        // The v2 page update needs the next version number, and requires title,
        // status, and body on every PUT: resend the current values for any the
        // caller leaves unchanged.
        let current = self.get_page(id).map_err(TrackerError::from)?;
        let current_version = current.version.as_ref().map(|v| v.number).unwrap_or(1);
        let current_status = current
            .status
            .clone()
            .unwrap_or_else(|| "current".to_string());
        let body = match &update.content {
            Some(content) => Some(markdown_to_storage(content)),
            None => current.body.and_then(|b| b.storage).and_then(|s| s.value),
        };

        let update_request = UpdateConfluencePage {
            id: id.to_string(),
            title: Some(update.summary.clone().unwrap_or(current.title)),
            body: body.map(|value| CreateConfluenceBody {
                representation: "storage".to_string(),
                value,
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

    fn move_article(&self, _article_id: &str, _new_parent_id: Option<&str>) -> Result<Article> {
        // Re-parenting needs Confluence's move API, which is not wired up. A
        // plain page update leaves the parent unchanged, so report the gap
        // rather than claim a move (to root or elsewhere) that never happened.
        Err(TrackerError::Unsupported(
            "Confluence does not support `article move` yet, with or without --parent; \
             move the page in Confluence instead"
                .to_string(),
        ))
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

    let created = parse_jira_datetime(&page.created_at).unwrap_or_else(Utc::now);

    let updated = page
        .version
        .as_ref()
        .and_then(|v| parse_jira_datetime(&v.created_at))
        .unwrap_or(created);

    Article {
        id: page.id.clone(),
        id_readable: page.id.clone(), // Confluence uses numeric IDs
        summary: page.title,
        content,
        project: ProjectRef {
            id: page.space_id.unwrap_or_default(),
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
    let created = parse_jira_datetime(&att.created_at);

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
        .and_then(|v| parse_jira_datetime(&v.when));

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

    let created = parse_jira_datetime(&comment.created_at);

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tracker_core::{AttachmentUploadFile, KnowledgeBase};
    use wiremock::matchers::{
        any, body_partial_json, body_string_contains, header, method, path, query_param,
        query_param_is_missing,
    };
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn page(id: &str, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "title": title,
            "spaceId": "SPACE"
        })
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

    #[tokio::test]
    async fn list_all_articles_walks_cursor_pages_once_and_truncates() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages"))
            .and(query_param("limit", "3"))
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
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages"))
            .and(query_param("limit", "1"))
            .and(query_param("cursor", "next-page"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    page("3", "Third")
                ],
                "_links": {
                    "next": "/wiki/api/v2/pages?cursor=unused-page"
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let articles = client.list_all_articles(None, 3).unwrap();

        assert_eq!(articles.len(), 3);
        assert_eq!(articles[0].id, "1");
        assert_eq!(articles[1].id, "2");
        assert_eq!(articles[2].id, "3");
    }

    #[tokio::test]
    async fn update_article_resends_unchanged_title_and_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "42",
                "title": "Runbook",
                "status": "current",
                "version": { "number": 3 },
                "body": { "storage": { "representation": "storage", "value": "<p>Old body</p>" } }
            })))
            .mount(&mock_server)
            .await;

        // Confluence's v2 page update requires title and body on every PUT.
        Mock::given(method("PUT"))
            .and(path("/wiki/api/v2/pages/42"))
            .and(body_partial_json(serde_json::json!({
                "title": "Runbook",
                "status": "current",
                "body": { "representation": "storage", "value": "<p>New body</p>" },
                "version": { "number": 4 }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(page("42", "Runbook")))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("PUT"))
            .and(path("/wiki/api/v2/pages/42"))
            .and(body_partial_json(serde_json::json!({
                "title": "Renamed",
                "body": { "representation": "storage", "value": "<p>Old body</p>" }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(page("42", "Renamed")))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        client
            .update_article(
                "42",
                &UpdateArticle {
                    summary: None,
                    content: Some("New body".to_string()),
                    tags: vec![],
                },
            )
            .unwrap();
        client
            .update_article(
                "42",
                &UpdateArticle {
                    summary: Some("Renamed".to_string()),
                    content: None,
                    tags: vec![],
                },
            )
            .unwrap();
    }

    #[tokio::test]
    async fn move_article_to_root_is_rejected_instead_of_faked() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page("42", "Runbook")))
            .mount(&mock_server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page("42", "Runbook")))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let err = client.move_article("42", None).unwrap_err();

        assert!(matches!(err, TrackerError::Unsupported(_)), "got: {err:?}");
    }

    // ==================== Error and HTTP Plumbing Characterization ====================
    //
    // The Confluence client has its own request helpers, with a different error
    // extractor from Jira's and a 404 rewrite. These pin what the shared
    // tracker-core HTTP helpers must keep, checked through the KnowledgeBase
    // trait so they hold whether the client returns JiraError or TrackerError.

    fn temp_upload_file(name: &str, contents: &[u8]) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("track-confluence-upload-{nanos}-{name}"));
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn upload_of(path: PathBuf, mime_type: Option<&str>) -> AttachmentUpload {
        AttachmentUpload {
            files: vec![AttachmentUploadFile {
                path,
                name: None,
                mime_type: mime_type.map(str::to_string),
            }],
            comment: None,
            silent: false,
            minor_edit: false,
        }
    }

    fn new_article() -> CreateArticle {
        CreateArticle {
            project_id: "SPACE".to_string(),
            summary: "Runbook".to_string(),
            content: None,
            parent_article_id: None,
            tags: vec![],
        }
    }

    /// The status and message of an `Api` error; panics on any other variant.
    fn expect_api(err: TrackerError) -> (u16, String) {
        match err {
            TrackerError::Api { status, message } => (status, message),
            other => panic!("Expected Api error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_confluence_error_message_and_errors_array_joined() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages/42"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "message": "M",
                "errors": [{ "message": "E1" }, { "message": "E2" }]
            })))
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let err = client.get_article("42").unwrap_err();

        assert_eq!(expect_api(err), (400, "M; E1; E2".to_string()));
    }

    #[tokio::test]
    async fn test_confluence_error_body_fallbacks() {
        let mock_server = MockServer::start().await;
        let jira_style = r#"{"errorMessages":["A"],"errors":{"f":"B"}}"#;
        let nested_only = r#"{"errors":[{"message":{"translation":"T"}}]}"#;
        // Jira-style errorMessages/errors{} and v1's nested
        // errors[].message.translation are not read. Only an empty, non-JSON
        // body becomes "HTTP <status>"; other bodies pass through raw.
        let cases = [
            ("1", 400, jira_style, jira_style),
            ("2", 400, nested_only, nested_only),
            (
                "3",
                400,
                r#"{"message":"M","data":{"errors":[{"message":{"translation":"T"}}]}}"#,
                "M",
            ),
            ("4", 500, "", "HTTP 500"),
            ("5", 502, "Bad Gateway", "Bad Gateway"),
        ];

        for (id, status, body, _) in cases {
            Mock::given(method("GET"))
                .and(path(format!("/wiki/api/v2/pages/{id}")))
                .respond_with(ResponseTemplate::new(status).set_body_string(body))
                .mount(&mock_server)
                .await;
        }

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        for (id, status, _, message) in cases {
            let err = client.get_article(id).unwrap_err();
            assert_eq!(expect_api(err), (status, message.to_string()), "page {id}");
        }
    }

    #[tokio::test]
    async fn test_confluence_404_is_not_found_message() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages/123"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "Page 123 missing"
            })))
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let err = client.get_article("123").unwrap_err();

        // The server's message is replaced (Jira keeps its 404 text).
        assert_eq!(expect_api(err), (404, "Not found".to_string()));
    }

    #[tokio::test]
    async fn test_confluence_401_is_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages/42"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Token expired"
            })))
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let err = client.get_article("42").unwrap_err();

        assert!(
            matches!(err, TrackerError::Unauthorized),
            "expected Unauthorized, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_confluence_non_json_success_body_is_parse() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/wiki/api/v2/pages/42"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let err = client.get_article("42").unwrap_err();

        // Agreed change: a body that is not the expected JSON is a Parse error.
        // Before the shared HTTP helpers it was Http("json: ...").
        assert!(
            matches!(err, TrackerError::Parse(_)),
            "expected Parse, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_confluence_post_and_upload_non_json_success_body_is_parse() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");

        Mock::given(method("POST"))
            .and(path("/wiki/api/v2/pages"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/wiki/rest/api/content/42/child/attachment"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let create_err = client.create_article(&new_article()).unwrap_err();
        let upload = upload_of(upload_path.clone(), None);
        let upload_err = client.add_article_attachment("42", &upload).unwrap_err();
        let _ = std::fs::remove_file(upload_path);

        // Agreed change, as for GET: send_json and the multipart upload both
        // decode their response.
        for err in [create_err, upload_err] {
            assert!(
                matches!(err, TrackerError::Parse(_)),
                "expected Parse, got: {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_confluence_delete_accepts_non_json_success_body() {
        let mock_server = MockServer::start().await;

        // delete_page discards the body, unlike every Confluence POST and PUT.
        Mock::given(method("DELETE"))
            .and(path("/wiki/api/v2/pages/42"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        client.delete_article("42").unwrap();
    }

    #[tokio::test]
    async fn test_confluence_send_json_headers_and_wiki_base_url() {
        let mock_server = MockServer::start().await;

        // Trimmed Basic auth, Accept: application/json, and Content-Type
        // without ureq's "; charset=utf-8".
        Mock::given(method("POST"))
            .and(path("/wiki/api/v2/pages"))
            .and(header(
                "Authorization",
                "Basic dGVzdEBleGFtcGxlLmNvbTp0b2tlbg==",
            ))
            .and(header("Accept", "application/json"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page("42", "Runbook")))
            .expect(1)
            .mount(&mock_server)
            .await;

        // A base URL that already ends in /wiki (here with a trailing slash)
        // does not get a second /wiki.
        let base_url = format!("{}/wiki/", mock_server.uri());
        let client = ConfluenceClient::new(&base_url, " test@example.com\n", "\ttoken ");
        let article = client.create_article(&new_article()).unwrap();

        assert_eq!(article.id, "42");
    }

    #[tokio::test]
    async fn test_confluence_upload_sends_comment_minor_edit_and_nocheck() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");

        // v1 endpoint, "nocheck" (Jira spells it "no-check"), and text parts
        // for the comment and minorEdit.
        Mock::given(method("POST"))
            .and(path("/wiki/rest/api/content/42/child/attachment"))
            .and(header("X-Atlassian-Token", "nocheck"))
            .and(header(
                "Authorization",
                "Basic dGVzdEBleGFtcGxlLmNvbTp0b2tlbg==",
            ))
            .and(header("Accept", "application/json"))
            .and(body_string_contains("name=\"file\""))
            .and(body_string_contains("name=\"comment\"\r\n\r\nc\r\n"))
            .and(body_string_contains("name=\"minorEdit\"\r\n\r\ntrue\r\n"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [
                    {
                        "id": "att1",
                        "title": "evidence.txt",
                        "metadata": {
                            "mediaType": "text/plain",
                            "extensions": { "fileSize": 8 }
                        }
                    }
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let upload = AttachmentUpload {
            comment: Some("c".to_string()),
            minor_edit: true,
            ..upload_of(upload_path.clone(), None)
        };
        let attachments = client.add_article_attachment("42", &upload).unwrap();
        let _ = std::fs::remove_file(upload_path);

        // The {results:[..]} wrapper is unwrapped.
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].id, "att1");
        assert_eq!(attachments[0].name, "evidence.txt");
        assert_eq!(attachments[0].mime_type.as_deref(), Some("text/plain"));
        assert_eq!(attachments[0].size, 8);
    }

    #[tokio::test]
    async fn test_confluence_attachment_missing_file_is_io_and_bad_mime_is_http() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");
        let missing_path = upload_path.with_extension("missing");

        // Both fail while building the form, before any request.
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": []
            })))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = ConfluenceClient::new(&mock_server.uri(), "test@example.com", "token");
        let missing_err = client
            .add_article_attachment("42", &upload_of(missing_path, None))
            .unwrap_err();
        let mime_upload = upload_of(upload_path.clone(), Some("not a mime"));
        let mime_err = client
            .add_article_attachment("42", &mime_upload)
            .unwrap_err();
        let _ = std::fs::remove_file(upload_path);

        assert!(
            matches!(missing_err, TrackerError::Io(_)),
            "expected Io, got: {missing_err:?}"
        );
        match mime_err {
            TrackerError::Http(message) => {
                assert!(message.starts_with("invalid MIME type"), "{message}");
            }
            other => panic!("Expected Http error, got: {other:?}"),
        }
    }
}
