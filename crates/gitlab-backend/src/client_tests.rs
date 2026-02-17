//! Unit tests for GitLabClient using wiremock

#[cfg(test)]
mod tests {
    use crate::client::GitLabClient;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Helper to create a mock GitLab issue response
    fn mock_gitlab_issue(iid: u64, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": 1000 + iid,
            "iid": iid,
            "project_id": 123,
            "title": title,
            "description": "Test description",
            "state": "opened",
            "labels": ["bug", "urgent"],
            "assignee": {
                "id": 1,
                "username": "testuser",
                "name": "Test User"
            },
            "assignees": [{
                "id": 1,
                "username": "testuser",
                "name": "Test User"
            }],
            "milestone": {
                "id": 1,
                "iid": 1,
                "title": "v1.0"
            },
            "created_at": "2024-01-01T00:00:00.000Z",
            "updated_at": "2024-01-02T00:00:00.000Z",
            "closed_at": null,
            "author": {
                "id": 2,
                "username": "reporter",
                "name": "Reporter Name"
            },
            "web_url": "https://gitlab.com/group/project/-/issues/42"
        })
    }

    /// Helper to create a mock GitLab project response
    fn mock_gitlab_project(id: u64, name: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "name": name,
            "name_with_namespace": format!("Group / {}", name),
            "path": name.to_lowercase().replace(' ', "-"),
            "path_with_namespace": format!("group/{}", name.to_lowercase().replace(' ', "-")),
            "description": "Project description",
            "web_url": format!("https://gitlab.com/group/{}", name.to_lowercase().replace(' ', "-"))
        })
    }

    #[tokio::test]
    async fn test_get_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_gitlab_issue(42, "Found a bug")),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let issue = client.get_issue(42).unwrap();

        assert_eq!(issue.iid, 42);
        assert_eq!(issue.title, "Found a bug");
        assert_eq!(issue.state, "opened");
        assert_eq!(issue.labels, vec!["bug", "urgent"]);
        assert!(issue.assignee.is_some());
        assert_eq!(issue.assignee.unwrap().username, "testuser");
    }

    #[tokio::test]
    async fn test_list_issues() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                mock_gitlab_issue(1, "First issue"),
                mock_gitlab_issue(2, "Second issue")
            ])))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let issues = client.list_issues(None, 20, 1).unwrap();

        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].iid, 1);
        assert_eq!(issues[0].title, "First issue");
        assert_eq!(issues[1].iid, 2);
        assert_eq!(issues[1].title, "Second issue");
    }

    #[tokio::test]
    async fn test_search_issues() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([mock_gitlab_issue(5, "Bug in login")])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let issues = client
            .search_issues("login", Some("opened"), None, 20, 1)
            .unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].title, "Bug in login");
    }

    #[tokio::test]
    async fn test_create_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(mock_gitlab_issue(99, "New issue")),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let create = crate::models::CreateGitLabIssue {
            title: "New issue".to_string(),
            description: Some("A new bug".to_string()),
            labels: Some("bug".to_string()),
            assignee_ids: None,
            milestone_id: None,
        };

        let issue = client.create_issue(&create).unwrap();
        assert_eq!(issue.iid, 99);
        assert_eq!(issue.title, "New issue");
    }

    #[tokio::test]
    async fn test_update_issue_uses_put() {
        let mock_server = MockServer::start().await;

        // Verify PUT method is used (not PATCH)
        Mock::given(method("PUT"))
            .and(path("/projects/123/issues/42"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_gitlab_issue(42, "Updated title")),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let update = crate::models::UpdateGitLabIssue {
            title: Some("Updated title".to_string()),
            ..Default::default()
        };

        let issue = client.update_issue(42, &update).unwrap();
        assert_eq!(issue.title, "Updated title");
    }

    #[tokio::test]
    async fn test_delete_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/projects/123/issues/42"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = client.delete_issue(42);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_projects() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                mock_gitlab_project(123, "Project Alpha"),
                mock_gitlab_project(456, "Project Beta")
            ])))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let projects = client.list_projects().unwrap();

        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "Project Alpha");
        assert_eq!(projects[1].name, "Project Beta");
    }

    #[tokio::test]
    async fn test_list_labels() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/labels"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": 1, "name": "bug", "color": "#fc2929", "description": "Something is broken"},
                {"id": 2, "name": "feature", "color": "#44ad8e", "description": "New feature"}
            ])))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let labels = client.list_labels().unwrap();

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].name, "bug");
        assert_eq!(labels[0].color, "#fc2929");
        assert_eq!(labels[1].name, "feature");
    }

    #[tokio::test]
    async fn test_add_note() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/projects/123/issues/42/notes"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 501,
                "body": "This is a comment",
                "author": {
                    "id": 1,
                    "username": "testuser",
                    "name": "Test User"
                },
                "created_at": "2024-01-15T14:00:00.000Z",
                "updated_at": "2024-01-15T14:00:00.000Z",
                "system": false
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let note = client.add_note(42, "This is a comment").unwrap();

        assert_eq!(note.id, 501);
        assert_eq!(note.body, "This is a comment");
        assert!(!note.system);
    }

    #[tokio::test]
    async fn test_get_notes_filters_system_notes() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42/notes"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 501,
                    "body": "User comment",
                    "author": {"id": 1, "username": "user", "name": "User"},
                    "created_at": "2024-01-15T10:00:00.000Z",
                    "updated_at": "2024-01-15T10:00:00.000Z",
                    "system": false
                },
                {
                    "id": 502,
                    "body": "changed the description",
                    "author": {"id": 2, "username": "admin", "name": "Admin"},
                    "created_at": "2024-01-15T11:00:00.000Z",
                    "updated_at": "2024-01-15T11:00:00.000Z",
                    "system": true
                },
                {
                    "id": 503,
                    "body": "Another user comment",
                    "author": {"id": 3, "username": "user2", "name": "User Two"},
                    "created_at": "2024-01-15T12:00:00.000Z",
                    "updated_at": "2024-01-15T12:00:00.000Z",
                    "system": false
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let notes = client.get_notes(42).unwrap();

        // System note (id=502) should be filtered out
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].id, 501);
        assert_eq!(notes[0].body, "User comment");
        assert_eq!(notes[1].id, 503);
        assert_eq!(notes[1].body, "Another user comment");
    }

    #[tokio::test]
    async fn test_get_issue_links() {
        let mock_server = MockServer::start().await;

        // GET /issues/:iid/links returns flat issue objects with link metadata
        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42/links"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 1043,
                    "iid": 43,
                    "title": "Related issue",
                    "issue_link_id": 1,
                    "link_type": "relates_to"
                },
                {
                    "id": 1044,
                    "iid": 44,
                    "title": "Blocked issue",
                    "issue_link_id": 2,
                    "link_type": "blocks"
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let links = client.get_issue_links(42).unwrap();

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].link_type, "relates_to");
        assert_eq!(links[0].iid, 43);
        assert_eq!(links[0].issue_link_id, 1);
        assert_eq!(links[1].link_type, "blocks");
        assert_eq!(links[1].iid, 44);
    }

    #[tokio::test]
    async fn test_create_label() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/projects/123/labels"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 42,
                "name": "new-label",
                "color": "#ededed",
                "description": "A new label"
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let create = crate::models::CreateGitLabLabel {
            name: "new-label".to_string(),
            color: "#ededed".to_string(),
            description: Some("A new label".to_string()),
        };

        let label = client.create_label(&create).unwrap();
        assert_eq!(label.id, 42);
        assert_eq!(label.name, "new-label");
        assert_eq!(label.color, "#ededed");
        assert_eq!(label.description, Some("A new label".to_string()));
    }

    #[tokio::test]
    async fn test_delete_label() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/projects/123/labels/42"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = client.delete_label(42);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_label() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(path("/projects/123/labels/42"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "name": "renamed-label",
                "color": "#ff0000",
                "description": "Updated description"
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let update = crate::models::UpdateGitLabLabel {
            new_name: Some("renamed-label".to_string()),
            color: Some("#ff0000".to_string()),
            description: Some("Updated description".to_string()),
        };

        let label = client.update_label(42, &update).unwrap();
        assert_eq!(label.id, 42);
        assert_eq!(label.name, "renamed-label");
        assert_eq!(label.color, "#ff0000");
        assert_eq!(label.description, Some("Updated description".to_string()));
    }

    #[tokio::test]
    async fn test_unauthorized_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "401 Unauthorized"
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "bad-token", Some("123"));
        let result = client.get_issue(42);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::GitLabError::Unauthorized
        ));
    }

    #[tokio::test]
    async fn test_project_url_requires_project_id() {
        let mock_server = MockServer::start().await;

        // Client created without project_id
        let client = GitLabClient::new(&mock_server.uri(), "test-token", None);

        // Operations that need project_id should fail
        let result = client.get_issue(42);
        assert!(result.is_err());
    }
}
