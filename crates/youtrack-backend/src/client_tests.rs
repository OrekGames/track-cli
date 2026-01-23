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
}
