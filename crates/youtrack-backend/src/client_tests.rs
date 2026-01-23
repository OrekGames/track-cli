#[cfg(test)]
mod tests {
    use crate::client::YouTrackClient;
    use crate::models::*;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_get_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/issues/PROJ-123"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "2-45",
                "idReadable": "PROJ-123",
                "summary": "Test issue",
                "description": "Test description",
                "project": {
                    "id": "0-1",
                    "name": "Test Project",
                    "shortName": "PROJ"
                },
                "customFields": [],
                "created": 1640000000000i64,
                "updated": 1640000000000i64,
                "$type": "Issue"
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let issue = client.get_issue("PROJ-123").unwrap();

        assert_eq!(issue.id_readable, "PROJ-123");
        assert_eq!(issue.summary, "Test issue");
        assert_eq!(issue.description, Some("Test description".to_string()));
    }

    #[tokio::test]
    async fn test_search_issues() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/issues"))
            .and(query_param("query", "project: PROJ"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "2-45",
                    "idReadable": "PROJ-123",
                    "summary": "First issue",
                    "project": {
                        "id": "0-1",
                        "shortName": "PROJ"
                    },
                    "customFields": [],
                    "created": 1640000000000i64,
                    "updated": 1640000000000i64,
                },
                {
                    "id": "2-46",
                    "idReadable": "PROJ-124",
                    "summary": "Second issue",
                    "project": {
                        "id": "0-1",
                        "shortName": "PROJ"
                    },
                    "customFields": [],
                    "created": 1640000000000i64,
                    "updated": 1640000000000i64,
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let issues = client.search_issues("project: PROJ", 20, 0).unwrap();

        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].id_readable, "PROJ-123");
        assert_eq!(issues[1].id_readable, "PROJ-124");
    }

    #[tokio::test]
    async fn test_create_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/issues"))
            .and(header("Authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "project": { "id": "0-1" },
                "summary": "New issue",
                "description": "New description"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "2-47",
                "idReadable": "PROJ-125",
                "summary": "New issue",
                "description": "New description",
                "project": {
                    "id": "0-1",
                    "shortName": "PROJ"
                },
                "customFields": [],
                "created": 1640000000000i64,
                "updated": 1640000000000i64,
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let create = CreateIssue {
            project: ProjectIdentifier {
                id: "0-1".to_string(),
            },
            summary: "New issue".to_string(),
            description: Some("New description".to_string()),
            custom_fields: vec![],
            tags: vec![],
        };

        let issue = client.create_issue(&create).unwrap();
        assert_eq!(issue.id_readable, "PROJ-125");
        assert_eq!(issue.summary, "New issue");
    }

    #[tokio::test]
    async fn test_update_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/issues/PROJ-123"))
            .and(header("Authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "summary": "Updated summary",
                "description": "Updated description"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "2-45",
                "idReadable": "PROJ-123",
                "summary": "Updated summary",
                "description": "Updated description",
                "project": {
                    "id": "0-1",
                    "shortName": "PROJ"
                },
                "customFields": [],
                "created": 1640000000000i64,
                "updated": 1640100000000i64,
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let update = UpdateIssue {
            summary: Some("Updated summary".to_string()),
            description: Some("Updated description".to_string()),
            custom_fields: vec![],
            tags: vec![],
        };

        let issue = client.update_issue("PROJ-123", &update).unwrap();
        assert_eq!(issue.summary, "Updated summary");
    }

    #[tokio::test]
    async fn test_delete_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/api/issues/PROJ-123"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let result = client.delete_issue("PROJ-123");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_projects() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/admin/projects"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "0-1",
                    "name": "Test Project",
                    "shortName": "PROJ",
                    "description": "A test project"
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let projects = client.list_projects().unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].short_name, "PROJ");
        assert_eq!(projects[0].name, "Test Project");
    }

    #[tokio::test]
    async fn test_unauthorized_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/issues/PROJ-123"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "bad-token");
        let result = client.get_issue("PROJ-123");

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::YouTrackError::Unauthorized
        ));
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/issues/NONEXISTENT"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let result = client.get_issue("NONEXISTENT");

        assert!(result.is_err());
    }

    // Article API tests

    #[tokio::test]
    async fn test_get_article() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/articles/KB-A-1"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "123-456",
                "idReadable": "KB-A-1",
                "summary": "Test Article",
                "content": "Article content here",
                "project": {
                    "id": "0-1",
                    "name": "Test Project",
                    "shortName": "KB"
                },
                "hasChildren": false,
                "tags": [],
                "created": 1640000000000i64,
                "updated": 1640000000000i64,
                "$type": "Article"
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let article = client.get_article("KB-A-1").unwrap();

        assert_eq!(article.id_readable.as_deref(), Some("KB-A-1"));
        assert_eq!(article.summary, "Test Article");
        assert_eq!(article.content, Some("Article content here".to_string()));
    }

    #[tokio::test]
    async fn test_list_articles() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/articles"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "123-456",
                    "idReadable": "KB-A-1",
                    "summary": "First Article",
                    "project": {
                        "id": "0-1",
                        "shortName": "KB"
                    },
                    "hasChildren": false,
                    "tags": [],
                    "created": 1640000000000i64,
                    "updated": 1640000000000i64
                },
                {
                    "id": "123-457",
                    "idReadable": "KB-A-2",
                    "summary": "Second Article",
                    "project": {
                        "id": "0-1",
                        "shortName": "KB"
                    },
                    "hasChildren": true,
                    "tags": [],
                    "created": 1640000000000i64,
                    "updated": 1640000000000i64
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let articles = client.list_articles(20, 0).unwrap();

        assert_eq!(articles.len(), 2);
        assert_eq!(articles[0].id_readable.as_deref(), Some("KB-A-1"));
        assert_eq!(articles[1].id_readable.as_deref(), Some("KB-A-2"));
        assert!(articles[1].has_children.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_search_articles() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/articles"))
            .and(query_param("query", "project: KB"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "123-456",
                    "idReadable": "KB-A-1",
                    "summary": "Matching Article",
                    "project": {
                        "id": "0-1",
                        "shortName": "KB"
                    },
                    "hasChildren": false,
                    "tags": [],
                    "created": 1640000000000i64,
                    "updated": 1640000000000i64
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let articles = client.search_articles("project: KB", 20, 0).unwrap();

        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].summary, "Matching Article");
    }

    #[tokio::test]
    async fn test_create_article() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/articles"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "123-458",
                "idReadable": "KB-A-3",
                "summary": "New Article",
                "content": "New article content",
                "project": {
                    "id": "0-1",
                    "shortName": "KB"
                },
                "hasChildren": false,
                "tags": [],
                "created": 1640000000000i64,
                "updated": 1640000000000i64
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let create = article::CreateArticle {
            project: ProjectIdentifier {
                id: "0-1".to_string(),
            },
            summary: "New Article".to_string(),
            content: Some("New article content".to_string()),
            parent_article: None,
            tags: vec![],
        };

        let article = client.create_article(&create).unwrap();
        assert_eq!(article.id_readable.as_deref(), Some("KB-A-3"));
        assert_eq!(article.summary, "New Article");
    }

    #[tokio::test]
    async fn test_update_article() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/articles/KB-A-1"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "123-456",
                "idReadable": "KB-A-1",
                "summary": "Updated Article",
                "content": "Updated content",
                "project": {
                    "id": "0-1",
                    "shortName": "KB"
                },
                "hasChildren": false,
                "tags": [],
                "created": 1640000000000i64,
                "updated": 1640100000000i64
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let update = article::UpdateArticle {
            summary: Some("Updated Article".to_string()),
            content: Some("Updated content".to_string()),
            tags: vec![],
        };

        let article = client.update_article("KB-A-1", &update).unwrap();
        assert_eq!(article.summary, "Updated Article");
    }

    #[tokio::test]
    async fn test_delete_article() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/api/articles/KB-A-1"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let result = client.delete_article("KB-A-1");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_child_articles() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/articles/KB-A-1/childArticles"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "123-459",
                    "idReadable": "KB-A-1-1",
                    "summary": "Child Article 1",
                    "project": {
                        "id": "0-1",
                        "shortName": "KB"
                    },
                    "hasChildren": false,
                    "tags": [],
                    "created": 1640000000000i64,
                    "updated": 1640000000000i64
                },
                {
                    "id": "123-460",
                    "idReadable": "KB-A-1-2",
                    "summary": "Child Article 2",
                    "project": {
                        "id": "0-1",
                        "shortName": "KB"
                    },
                    "hasChildren": false,
                    "tags": [],
                    "created": 1640000000000i64,
                    "updated": 1640000000000i64
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let children = client.get_child_articles("KB-A-1").unwrap();

        assert_eq!(children.len(), 2);
        assert_eq!(children[0].id_readable.as_deref(), Some("KB-A-1-1"));
        assert_eq!(children[1].id_readable.as_deref(), Some("KB-A-1-2"));
    }

    #[tokio::test]
    async fn test_list_article_attachments() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/articles/KB-A-1/attachments"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "att-1",
                    "name": "document.pdf",
                    "size": 102400,
                    "mimeType": "application/pdf",
                    "created": 1640000000000i64
                },
                {
                    "id": "att-2",
                    "name": "image.png",
                    "size": 51200,
                    "mimeType": "image/png",
                    "created": 1640000000000i64
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let attachments = client.list_article_attachments("KB-A-1").unwrap();

        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0].name, "document.pdf");
        assert_eq!(attachments[0].size, 102400);
        assert_eq!(attachments[1].name, "image.png");
    }

    #[tokio::test]
    async fn test_get_article_comments() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/articles/KB-A-1/comments"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "comment-1",
                    "text": "First comment",
                    "author": {
                        "login": "user1",
                        "name": "User One"
                    },
                    "created": 1640000000000i64
                },
                {
                    "id": "comment-2",
                    "text": "Second comment",
                    "author": {
                        "login": "user2",
                        "name": "User Two"
                    },
                    "created": 1640100000000i64
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let comments = client.get_article_comments("KB-A-1").unwrap();

        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].text, "First comment");
        assert_eq!(comments[1].text, "Second comment");
    }

    #[tokio::test]
    async fn test_add_article_comment() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/articles/KB-A-1/comments"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "comment-3",
                "text": "New comment",
                "author": {
                    "login": "user1",
                    "name": "User One"
                },
                "created": 1640200000000i64
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let comment = client.add_article_comment("KB-A-1", "New comment").unwrap();

        assert_eq!(comment.text, "New comment");
    }

    #[tokio::test]
    async fn test_article_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/articles/NONEXISTENT"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let result = client.get_article("NONEXISTENT");

        assert!(result.is_err());
    }
}
