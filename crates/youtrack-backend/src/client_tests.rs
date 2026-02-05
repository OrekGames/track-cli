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

    // ========== Custom Field Admin API Tests ==========

    #[tokio::test]
    async fn test_list_custom_field_definitions() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/admin/customFieldSettings/customFields"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "127-1",
                    "name": "Priority",
                    "fieldType": {
                        "id": "enum[1]",
                        "presentation": "enum"
                    }
                },
                {
                    "id": "127-2",
                    "name": "State",
                    "fieldType": {
                        "id": "state[1]",
                        "presentation": "state"
                    }
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let fields = client.list_custom_field_definitions().unwrap();

        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "Priority");
        assert_eq!(fields[0].field_type.id, "enum[1]");
        assert_eq!(fields[1].name, "State");
        assert_eq!(fields[1].field_type.id, "state[1]");
    }

    #[tokio::test]
    async fn test_create_custom_field() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/admin/customFieldSettings/customFields"))
            .and(header("Authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "name": "Test Field",
                "fieldType": {
                    "id": "enum[1]"
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "127-10",
                "name": "Test Field",
                "fieldType": {
                    "id": "enum[1]",
                    "presentation": "enum"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let create = admin::CreateCustomFieldRequest {
            name: "Test Field".to_string(),
            field_type: admin::FieldTypeRef {
                id: "enum[1]".to_string(),
            },
        };

        let field = client.create_custom_field(&create).unwrap();
        assert_eq!(field.id, "127-10");
        assert_eq!(field.name, "Test Field");
    }

    #[tokio::test]
    async fn test_list_bundles() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/admin/customFieldSettings/bundles/enum"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "$type": "EnumBundle",
                    "id": "129-1",
                    "name": "Priority Bundle",
                    "values": [
                        {"id": "129-1-1", "name": "Low", "ordinal": 0},
                        {"id": "129-1-2", "name": "Medium", "ordinal": 1},
                        {"id": "129-1-3", "name": "High", "ordinal": 2}
                    ]
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let bundles = client.list_bundles("enum").unwrap();

        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].name, "Priority Bundle");
        assert_eq!(bundles[0].bundle_type, "EnumBundle");
        assert_eq!(bundles[0].values.len(), 3);
        assert_eq!(bundles[0].values[0].name, "Low");
    }

    #[tokio::test]
    async fn test_list_state_bundles() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/admin/customFieldSettings/bundles/state"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "$type": "StateBundle",
                    "id": "131-1",
                    "name": "Issue Status",
                    "values": [
                        {"id": "131-1-1", "name": "Open", "isResolved": false, "ordinal": 0},
                        {"id": "131-1-2", "name": "In Progress", "isResolved": false, "ordinal": 1},
                        {"id": "131-1-3", "name": "Done", "isResolved": true, "ordinal": 2}
                    ]
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let bundles = client.list_bundles("state").unwrap();

        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].bundle_type, "StateBundle");
        assert_eq!(bundles[0].values[0].is_resolved, Some(false));
        assert_eq!(bundles[0].values[2].is_resolved, Some(true));
    }

    #[tokio::test]
    async fn test_create_bundle() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/admin/customFieldSettings/bundles/enum"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "$type": "EnumBundle",
                "id": "129-10",
                "name": "Test Priority",
                "values": [
                    {"id": "129-10-1", "name": "Low", "ordinal": 0},
                    {"id": "129-10-2", "name": "High", "ordinal": 1}
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let create = admin::CreateBundleRequest {
            name: "Test Priority".to_string(),
            values: vec![
                admin::CreateBundleValueRequest {
                    name: "Low".to_string(),
                    description: None,
                    is_resolved: None,
                    ordinal: Some(0),
                },
                admin::CreateBundleValueRequest {
                    name: "High".to_string(),
                    description: None,
                    is_resolved: None,
                    ordinal: Some(1),
                },
            ],
        };

        let bundle = client.create_bundle("enum", &create).unwrap();
        assert_eq!(bundle.id, "129-10");
        assert_eq!(bundle.name, "Test Priority");
        assert_eq!(bundle.values.len(), 2);
    }

    #[tokio::test]
    async fn test_create_state_bundle() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/admin/customFieldSettings/bundles/state"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "$type": "StateBundle",
                "id": "131-10",
                "name": "Test Status",
                "values": [
                    {"id": "131-10-1", "name": "Open", "isResolved": false, "ordinal": 0},
                    {"id": "131-10-2", "name": "Closed", "isResolved": true, "ordinal": 1}
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let create = admin::CreateBundleRequest {
            name: "Test Status".to_string(),
            values: vec![
                admin::CreateBundleValueRequest {
                    name: "Open".to_string(),
                    description: None,
                    is_resolved: Some(false),
                    ordinal: Some(0),
                },
                admin::CreateBundleValueRequest {
                    name: "Closed".to_string(),
                    description: None,
                    is_resolved: Some(true),
                    ordinal: Some(1),
                },
            ],
        };

        let bundle = client.create_bundle("state", &create).unwrap();
        assert_eq!(bundle.bundle_type, "StateBundle");
        assert_eq!(bundle.values[0].is_resolved, Some(false));
        assert_eq!(bundle.values[1].is_resolved, Some(true));
    }

    #[tokio::test]
    async fn test_add_bundle_value() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path(
                "/api/admin/customFieldSettings/bundles/enum/129-10/values",
            ))
            .and(header("Authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "name": "Critical",
                "ordinal": 2
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "129-10-3",
                "name": "Critical",
                "ordinal": 2
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let value = admin::CreateBundleValueRequest {
            name: "Critical".to_string(),
            description: None,
            is_resolved: None,
            ordinal: Some(2),
        };

        let created = client.add_bundle_value("enum", "129-10", &value).unwrap();
        assert_eq!(created.id, "129-10-3");
        assert_eq!(created.name, "Critical");
    }

    #[tokio::test]
    async fn test_attach_field_to_project() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/admin/projects/0-1/customFields"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "128-1",
                "field": {
                    "id": "127-10",
                    "name": "Test Field",
                    "fieldType": {
                        "id": "enum[1]",
                        "presentation": "enum"
                    }
                },
                "canBeEmpty": true,
                "bundle": {
                    "id": "129-10",
                    "values": [
                        {"id": "129-10-1", "name": "Low"},
                        {"id": "129-10-2", "name": "High"}
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let attach = admin::AttachFieldRequest {
            type_name: "EnumProjectCustomField".to_string(),
            field: admin::CustomFieldRef {
                id: "127-10".to_string(),
            },
            bundle: Some(admin::BundleRef {
                type_name: "EnumBundle".to_string(),
                id: "129-10".to_string(),
            }),
            can_be_empty: true,
            empty_field_text: None,
        };

        let attached = client.attach_field_to_project("0-1", &attach).unwrap();
        assert_eq!(attached.id, "128-1");
        assert_eq!(attached.field.name, "Test Field");
        assert!(attached.can_be_empty);
        assert!(attached.bundle.is_some());
    }

    // ========== Error Response Body Parsing Tests ==========

    #[tokio::test]
    async fn test_bad_request_with_json_error_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/issues"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "Bad Request",
                "error_description": "Missing required field: Priority"
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let create = CreateIssue {
            project: ProjectIdentifier {
                id: "0-1".to_string(),
            },
            summary: "Test".to_string(),
            description: None,
            custom_fields: vec![],
            tags: vec![],
        };

        let result = client.create_issue(&create);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err {
            crate::error::YouTrackError::Api { status, message } => {
                assert_eq!(*status, 400);
                assert!(
                    message.contains("Missing required field: Priority"),
                    "Error message should contain field validation detail, got: {message}"
                );
            }
            other => panic!("Expected Api error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_bad_request_with_error_message_field() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/issues"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error_message": "Invalid value for field 'Type'"
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let create = CreateIssue {
            project: ProjectIdentifier {
                id: "0-1".to_string(),
            },
            summary: "Test".to_string(),
            description: None,
            custom_fields: vec![],
            tags: vec![],
        };

        let result = client.create_issue(&create);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err {
            crate::error::YouTrackError::Api { status, message } => {
                assert_eq!(*status, 400);
                assert!(
                    message.contains("Invalid value for field 'Type'"),
                    "Error message should contain error_message content, got: {message}"
                );
            }
            other => panic!("Expected Api error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_bad_request_with_plain_text_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/issues"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_string("Something went wrong on the server side"),
            )
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let create = CreateIssue {
            project: ProjectIdentifier {
                id: "0-1".to_string(),
            },
            summary: "Test".to_string(),
            description: None,
            custom_fields: vec![],
            tags: vec![],
        };

        let result = client.create_issue(&create);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err {
            crate::error::YouTrackError::Api { status, message } => {
                assert_eq!(*status, 400);
                assert!(
                    message.contains("Something went wrong"),
                    "Error message should contain raw body text, got: {message}"
                );
            }
            other => panic!("Expected Api error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_not_found_with_json_error_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/issues/PROJ-99999"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": "Not Found",
                "error_description": "Entity with id PROJ-99999 not found"
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let result = client.get_issue("PROJ-99999");

        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            crate::error::YouTrackError::Api { status, message } => {
                assert_eq!(*status, 404);
                assert!(
                    message.contains("PROJ-99999 not found"),
                    "Error message should contain entity detail, got: {message}"
                );
            }
            other => panic!("Expected Api error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_unauthorized_with_body_still_returns_unauthorized_variant() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/issues/PROJ-123"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "Unauthorized",
                "error_description": "Invalid token"
            })))
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
    async fn test_bad_request_with_empty_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/issues"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let create = CreateIssue {
            project: ProjectIdentifier {
                id: "0-1".to_string(),
            },
            summary: "Test".to_string(),
            description: None,
            custom_fields: vec![],
            tags: vec![],
        };

        let result = client.create_issue(&create);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err {
            crate::error::YouTrackError::Api { status, message } => {
                assert_eq!(*status, 400);
                assert_eq!(message, "HTTP 400");
            }
            other => panic!("Expected Api error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_attach_state_field_to_project() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/admin/projects/0-1/customFields"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "128-2",
                "field": {
                    "id": "127-11",
                    "name": "Status",
                    "fieldType": {
                        "id": "state[1]",
                        "presentation": "state"
                    }
                },
                "canBeEmpty": false,
                "bundle": {
                    "id": "131-10",
                    "values": [
                        {"id": "131-10-1", "name": "Open", "isResolved": false},
                        {"id": "131-10-2", "name": "Done", "isResolved": true}
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let client = YouTrackClient::new(&mock_server.uri(), "test-token");
        let attach = admin::AttachFieldRequest {
            type_name: "StateProjectCustomField".to_string(),
            field: admin::CustomFieldRef {
                id: "127-11".to_string(),
            },
            bundle: Some(admin::BundleRef {
                type_name: "StateBundle".to_string(),
                id: "131-10".to_string(),
            }),
            can_be_empty: false,
            empty_field_text: None,
        };

        let attached = client.attach_field_to_project("0-1", &attach).unwrap();
        assert_eq!(attached.id, "128-2");
        assert!(!attached.can_be_empty);
    }
}
