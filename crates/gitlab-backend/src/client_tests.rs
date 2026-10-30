//! Unit tests for GitLabClient using wiremock

#[cfg(test)]
mod tests {
    use crate::client::GitLabClient;
    use crate::convert::GitLabQueryParams;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tracker_core::{
        AttachmentUpload, AttachmentUploadFile, CreateIssue, CustomFieldUpdate, IssueTracker,
        KnowledgeBase, TrackerError, UpdateIssue,
    };
    use wiremock::matchers::{
        any, body_partial_json, body_string_contains, header, method, path, query_param,
    };
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn temp_upload_file(name: &str, contents: &[u8]) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("track-gitlab-upload-{nanos}-{name}"));
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn params(search: &str, state: Option<&str>, assignee: Option<&str>) -> GitLabQueryParams {
        GitLabQueryParams {
            search: search.to_string(),
            state: state.map(str::to_string),
            assignee_username: assignee.map(str::to_string),
            ..Default::default()
        }
    }

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

    fn mock_wiki_page(slug: &str, title: &str, content: &str) -> serde_json::Value {
        serde_json::json!({
            "slug": slug,
            "title": title,
            "content": content,
            "format": "markdown",
            "encoding": "UTF-8"
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
        let (issues, _) = client
            .fetch_project_issues(&GitLabQueryParams::default(), 20, 1)
            .unwrap();

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
        let (issues, _) = client
            .fetch_project_issues(&params("login", Some("opened"), None), 20, 1)
            .unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].title, "Bug in login");
    }

    #[tokio::test]
    async fn test_trait_search_issues_preserves_offset() {
        let mock_server = MockServer::start().await;

        // Skip 125 starts on page 2 at offset 25; the window runs into the
        // short page 3, whose different X-Total must not replace the first.
        for (page, iids, total) in [("2", 101..=200, "230"), ("3", 201..=230, "999")] {
            let issues: Vec<_> = iids
                .map(|iid| mock_gitlab_issue(iid, &format!("Search result {iid}")))
                .collect();
            Mock::given(method("GET"))
                .and(path("/projects/123/issues"))
                .and(header("PRIVATE-TOKEN", "test-token"))
                .and(query_param("per_page", "100"))
                .and(query_param("page", page))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("x-total", total)
                        .set_body_json(issues),
                )
                .expect(1)
                .mount(&mock_server)
                .await;
        }

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = <GitLabClient as IssueTracker>::search_issues(&client, "", 100, 125).unwrap();

        assert_eq!(result.items.len(), 100);
        assert_eq!(result.items[0].id_readable, "#126");
        assert_eq!(result.items[99].id_readable, "#225");
        assert_eq!(result.total, Some(230));
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
            issue_type: None,
        };

        let issue = client.create_issue(&create).unwrap();
        assert_eq!(issue.iid, 99);
        assert_eq!(issue.title, "New issue");
    }

    #[tokio::test]
    async fn test_trait_issue_attachment_upload_creates_issue_note() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");

        Mock::given(method("POST"))
            .and(path("/projects/123/uploads"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(body_string_contains("name=\"file\""))
            .and(body_string_contains("filename=\"custom.txt\""))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "alt": "custom.txt",
                "url": "/uploads/custom.txt",
                "full_path": "/group/project/uploads/custom.txt",
                "markdown": "[custom.txt](/uploads/custom.txt)"
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/projects/123/issues/42/notes"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(body_string_contains("supporting material"))
            .and(body_string_contains("[custom.txt](/uploads/custom.txt)"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 5001,
                "body": "supporting material\n\n[custom.txt](/uploads/custom.txt)",
                "author": {
                    "id": 7,
                    "username": "maintainer",
                    "name": "Maintainer"
                },
                "created_at": "2024-01-01T00:00:00.000Z",
                "updated_at": "2024-01-01T00:00:00.000Z",
                "system": false
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let upload = AttachmentUpload {
            files: vec![AttachmentUploadFile {
                path: upload_path.clone(),
                name: Some("custom.txt".to_string()),
                mime_type: Some("text/plain".to_string()),
            }],
            comment: Some("supporting material".to_string()),
            silent: false,
            minor_edit: false,
        };

        let attachments =
            <GitLabClient as IssueTracker>::add_issue_attachment(&client, "42", &upload)
                .expect("GitLab fallback attachment upload should succeed");

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].name, "custom.txt");
        assert_eq!(attachments[0].comment_id.as_deref(), Some("5001"));
        assert_eq!(
            attachments[0].markdown.as_deref(),
            Some("[custom.txt](/uploads/custom.txt)")
        );
        let _ = std::fs::remove_file(upload_path);
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

    /// `CustomFieldUpdate::SingleUser` is the `--assignee` channel. GitLab write
    /// paths must harvest it into `assignee_ids` instead of dropping it.
    #[tokio::test]
    async fn trait_create_and_update_send_assignee_ids() {
        #[derive(Clone, Copy)]
        struct Case {
            name: &'static str,
            method: &'static str,
            path: &'static str,
        }

        let cases = [
            Case {
                name: "create",
                method: "POST",
                path: "/projects/123/issues",
            },
            Case {
                name: "update",
                method: "PUT",
                path: "/projects/123/issues/42",
            },
        ];

        let assignee = serde_json::json!({
            "id": 7,
            "username": "alice",
            "name": "Alice"
        });

        for case in cases {
            let mock_server = MockServer::start().await;

            // Resolution endpoints the honor-`assignee_ids` fix can use.
            Mock::given(method("GET"))
                .and(path("/projects/123/members/all"))
                .and(header("PRIVATE-TOKEN", "test-token"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!([assignee.clone()])),
                )
                .mount(&mock_server)
                .await;

            Mock::given(method("GET"))
                .and(path("/projects/123/members"))
                .and(header("PRIVATE-TOKEN", "test-token"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!([assignee.clone()])),
                )
                .mount(&mock_server)
                .await;

            Mock::given(method("GET"))
                .and(path("/users"))
                .and(header("PRIVATE-TOKEN", "test-token"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!([assignee.clone()])),
                )
                .mount(&mock_server)
                .await;

            Mock::given(method("GET"))
                .and(path("/projects/123/issues/42"))
                .and(header("PRIVATE-TOKEN", "test-token"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(mock_gitlab_issue(42, "Existing")),
                )
                .mount(&mock_server)
                .await;

            Mock::given(method(case.method))
                .and(path(case.path))
                .and(header("PRIVATE-TOKEN", "test-token"))
                .respond_with(
                    ResponseTemplate::new(if case.method == "POST" { 201 } else { 200 })
                        .set_body_json(mock_gitlab_issue(42, "Assigned issue")),
                )
                .mount(&mock_server)
                .await;

            let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
            let field = CustomFieldUpdate::SingleUser {
                name: "Assignee".to_string(),
                login: "alice".to_string(),
            };

            match case.name {
                "create" => {
                    let created = <GitLabClient as IssueTracker>::create_issue(
                        &client,
                        &CreateIssue {
                            project_id: "123".to_string(),
                            summary: "Assigned issue".to_string(),
                            description: None,
                            custom_fields: vec![field],
                            tags: vec![],
                            parent: None,
                        },
                    );
                    assert!(
                        created.is_ok(),
                        "{} should succeed when assignee is honored: {created:?}",
                        case.name
                    );
                }
                "update" => {
                    let updated = <GitLabClient as IssueTracker>::update_issue(
                        &client,
                        "42",
                        &UpdateIssue {
                            custom_fields: vec![field],
                            ..Default::default()
                        },
                    );
                    assert!(
                        updated.is_ok(),
                        "{} should succeed when assignee is honored: {updated:?}",
                        case.name
                    );
                }
                other => panic!("unexpected case {other}"),
            }

            let requests = mock_server.received_requests().await.unwrap();
            let write = requests.iter().find(|request| {
                request.method.as_str() == case.method && request.url.path() == case.path
            });
            let write = write.unwrap_or_else(|| {
                panic!(
                    "{} dropped assignee: expected {} {} with assignee_ids, received {:#?}",
                    case.name,
                    case.method,
                    case.path,
                    requests
                        .iter()
                        .map(|request| format!("{} {}", request.method, request.url.path()))
                        .collect::<Vec<_>>()
                )
            });

            let body: serde_json::Value = write.body_json().unwrap_or_else(|err| {
                panic!("{} write body is not JSON: {err}", case.name);
            });
            let ids = body.get("assignee_ids").and_then(|value| value.as_array());
            let ids = ids.unwrap_or_else(|| {
                panic!(
                    "{} silently dropped --assignee: write body missing assignee_ids: {body}",
                    case.name
                )
            });
            assert!(
                ids.iter().any(|id| id.as_u64() == Some(7)),
                "{} must send resolved assignee_ids including 7, got {ids:?}",
                case.name
            );
        }
    }

    /// Mount the resolution endpoints used by `find_user_id` for a member
    /// with a mixed-case username and a specific numeric id.
    async fn mount_user_resolution(mock_server: &MockServer, member: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path("/projects/123/members/all"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([member.clone()])),
            )
            .mount(mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/projects/123/members"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([member])))
            .mount(mock_server)
            .await;
    }

    async fn mount_issue_write(
        mock_server: &MockServer,
        http_method: &str,
        issue_path: &str,
        status: u16,
    ) {
        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_gitlab_issue(42, "Existing")),
            )
            .mount(mock_server)
            .await;
        Mock::given(method(http_method))
            .and(path(issue_path))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(mock_gitlab_issue(42, "Assigned")),
            )
            .mount(mock_server)
            .await;
    }

    /// `@username` is accepted and resolved case-insensitively via project
    /// membership (GitLab usernames are case-insensitive in practice).
    #[tokio::test]
    async fn trait_assignee_accepts_at_prefixed_and_mixed_case_username() {
        let mock_server = MockServer::start().await;
        mount_user_resolution(
            &mock_server,
            serde_json::json!({"id": 7, "username": "Alice", "name": "Alice"}),
        )
        .await;
        mount_issue_write(&mock_server, "POST", "/projects/123/issues", 201).await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let created = <GitLabClient as IssueTracker>::create_issue(
            &client,
            &CreateIssue {
                project_id: "123".to_string(),
                summary: "Assigned issue".to_string(),
                description: None,
                custom_fields: vec![CustomFieldUpdate::SingleUser {
                    name: "Assignee".to_string(),
                    login: "@ALICE".to_string(),
                }],
                tags: vec![],
                parent: None,
            },
        );
        assert!(created.is_ok(), "@ALICE should resolve: {created:?}");

        let requests = mock_server.received_requests().await.unwrap();
        let write = requests
            .iter()
            .find(|r| r.method.as_str() == "POST" && r.url.path() == "/projects/123/issues")
            .expect("create must be issued");
        let body: serde_json::Value = write.body_json().unwrap();
        assert_eq!(
            body["assignee_ids"],
            serde_json::json!([7]),
            "case-insensitive @username must resolve to the member id"
        );
    }

    /// A numeric assignee must be verified against `GET /users/:id`; the
    /// returned identity evidence must match, otherwise the operation fails
    /// instead of being silently dropped by GitLab.
    #[tokio::test]
    async fn trait_assignee_numeric_id_is_verified_and_rejected_when_unknown() {
        // Verified path: id 7 exists.
        let mock_server = MockServer::start().await;
        mount_issue_write(&mock_server, "PUT", "/projects/123/issues/42", 200).await;
        Mock::given(method("GET"))
            .and(path("/users/7"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 7,
                "username": "alice",
                "name": "Alice"
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let updated = <GitLabClient as IssueTracker>::update_issue(
            &client,
            "42",
            &UpdateIssue {
                custom_fields: vec![CustomFieldUpdate::SingleUser {
                    name: "Assignee".to_string(),
                    login: "7".to_string(),
                }],
                ..Default::default()
            },
        );
        assert!(
            updated.is_ok(),
            "verified numeric id must pass: {updated:?}"
        );
        let requests = mock_server.received_requests().await.unwrap();
        let write = requests
            .iter()
            .find(|r| r.method.as_str() == "PUT" && r.url.path() == "/projects/123/issues/42")
            .expect("update must be issued");
        let body: serde_json::Value = write.body_json().unwrap();
        assert_eq!(body["assignee_ids"], serde_json::json!([7]));

        // Unknown numeric id: verification 404s, no write is issued.
        let mock_server = MockServer::start().await;
        mount_issue_write(&mock_server, "PUT", "/projects/123/issues/42", 200).await;
        Mock::given(method("GET"))
            .and(path("/users/999999"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({
                        "message": "404 User Not Found"
                    }))
                    .append_header("content-type", "application/json"),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let updated = <GitLabClient as IssueTracker>::update_issue(
            &client,
            "42",
            &UpdateIssue {
                custom_fields: vec![CustomFieldUpdate::SingleUser {
                    name: "Assignee".to_string(),
                    login: "999999".to_string(),
                }],
                ..Default::default()
            },
        );
        assert!(
            updated.is_err(),
            "unknown numeric id must fail instead of being silently dropped: {updated:?}"
        );
        let requests = mock_server.received_requests().await.unwrap();
        assert!(
            !requests.iter().any(|r| r.method.as_str() == "PUT"),
            "no issue write may be issued when assignee verification fails"
        );
    }

    /// Empty string and "0" are explicit clear-assignee requests and must be
    /// forwarded as an empty `assignee_ids` array, not treated as a lookup.
    #[tokio::test]
    async fn trait_assignee_empty_and_zero_clear_assignees() {
        for login in ["", "0", "@0"] {
            let mock_server = MockServer::start().await;
            mount_issue_write(&mock_server, "PUT", "/projects/123/issues/42", 200).await;

            let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
            let updated = <GitLabClient as IssueTracker>::update_issue(
                &client,
                "42",
                &UpdateIssue {
                    custom_fields: vec![CustomFieldUpdate::SingleUser {
                        name: "Assignee".to_string(),
                        login: login.to_string(),
                    }],
                    ..Default::default()
                },
            );
            assert!(updated.is_ok(), "clear '{login}' must succeed: {updated:?}");
            let requests = mock_server.received_requests().await.unwrap();
            assert!(
                !requests.iter().any(|r| r.url.path().starts_with("/users")),
                "clear request '{login}' must not trigger a user lookup"
            );
            let write = requests
                .iter()
                .find(|r| r.method.as_str() == "PUT" && r.url.path() == "/projects/123/issues/42")
                .expect("update must be issued");
            let body: serde_json::Value = write.body_json().unwrap();
            assert_eq!(
                body["assignee_ids"],
                serde_json::json!([]),
                "clear request '{login}' must send an empty assignee_ids array"
            );
        }
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
    async fn test_get_comments_filters_system_notes() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42/notes"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "1"))
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
        let notes = client.get_comments("42").unwrap();

        // System note (id=502) should be filtered out
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].id, "501");
        assert_eq!(notes[0].text, "User comment");
        assert_eq!(notes[1].id, "503");
        assert_eq!(notes[1].text, "Another user comment");
    }

    /// Mount issue 42's notes as a full page 1 (ids 1..=100, where 50 is a
    /// system note) and a short page 2 (ids 101 and 102), each of which
    /// must be requested exactly once.
    async fn mount_two_note_pages(mock_server: &MockServer) {
        let pages = [("1", 1..=100), ("2", 101..=102)];
        for (page, ids) in pages {
            let notes: Vec<_> = ids
                .map(|id| {
                    serde_json::json!({
                        "id": id,
                        "body": format!("Comment {id}"),
                        "author": {"id": 1, "username": "user", "name": "User"},
                        "created_at": "2024-01-15T10:00:00.000Z",
                        "updated_at": "2024-01-15T10:00:00.000Z",
                        "system": id == 50
                    })
                })
                .collect();
            Mock::given(method("GET"))
                .and(path("/projects/123/issues/42/notes"))
                .and(header("PRIVATE-TOKEN", "test-token"))
                .and(query_param("per_page", "100"))
                .and(query_param("page", page))
                .respond_with(ResponseTemplate::new(200).set_body_json(notes))
                .expect(1)
                .mount(mock_server)
                .await;
        }
    }

    #[tokio::test]
    async fn test_get_all_comments_walks_native_note_pages_once() {
        let mock_server = MockServer::start().await;
        mount_two_note_pages(&mock_server).await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let comments = <GitLabClient as IssueTracker>::get_all_comments(&client, "#42", 101)
            .expect("get_all_comments should succeed");

        assert_eq!(comments.len(), 101);
        assert!(!comments.iter().any(|comment| comment.id == "50"));
        assert_eq!(comments[0].id, "1");
        assert_eq!(comments[100].id, "102");
    }

    #[tokio::test]
    async fn test_get_comments_page_skip_counts_visible_notes() {
        let mock_server = MockServer::start().await;
        mount_two_note_pages(&mock_server).await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let comments = <GitLabClient as IssueTracker>::get_comments_page(&client, "#42", 2, 98)
            .expect("get_comments_page should succeed");

        // Page 1 has 99 visible notes, so skipping 98 leaves only note 100
        // there; counting raw notes (system note 50 included) would return
        // notes 99 and 100 instead.
        let ids: Vec<_> = comments.iter().map(|comment| comment.id.as_str()).collect();
        assert_eq!(ids, ["100", "101"]);
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
    async fn test_project_url_requires_project_id() {
        let mock_server = MockServer::start().await;

        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock_server)
            .await;

        // Client created without project_id
        let client = GitLabClient::new(&mock_server.uri(), "test-token", None);

        // Operations that need project_id fail before any request, with a
        // synthetic status-0 Api error rather than InvalidInput.
        let err = client.get_issue(42).unwrap_err();
        let (status, message) = api_error(err);
        assert_eq!(status, 0);
        assert!(
            message.starts_with("GitLab project_id is not configured"),
            "unexpected message: {message}"
        );
    }

    #[tokio::test]
    async fn test_issue_count_reads_x_total_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("per_page", "1"))
            .and(query_param("page", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "847")
                    .set_body_json(serde_json::json!([])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let (_, total) = client
            .fetch_project_issues(&params("bug", Some("opened"), None), 1, 1)
            .unwrap();

        assert_eq!(total, Some(847));
    }

    #[tokio::test]
    async fn test_search_issues_reads_x_total_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("search", "bug"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "42")
                    .set_body_json(serde_json::json!([
                        mock_gitlab_issue(1, "Bug report"),
                        mock_gitlab_issue(2, "Another bug")
                    ])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let (issues, total) = client
            .fetch_project_issues(&params("bug", None, None), 20, 1)
            .unwrap();

        assert_eq!(issues.len(), 2);
        assert_eq!(total, Some(42));
    }

    #[tokio::test]
    async fn test_search_issues_without_x_total_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("search", "bug"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([mock_gitlab_issue(1, "Bug report")])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let (issues, total) = client
            .fetch_project_issues(&params("bug", None, None), 20, 1)
            .unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(total, None);
    }

    #[tokio::test]
    async fn test_list_issues_reads_x_total_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "100")
                    .set_body_json(serde_json::json!([
                        mock_gitlab_issue(1, "First issue"),
                        mock_gitlab_issue(2, "Second issue")
                    ])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let (issues, total) = client
            .fetch_project_issues(&params("", Some("opened"), None), 20, 1)
            .unwrap();

        assert_eq!(issues.len(), 2);
        assert_eq!(total, Some(100));
    }

    #[tokio::test]
    async fn test_list_issues_maps_me_to_assigned_to_me_scope() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("state", "opened"))
            .and(query_param("scope", "assigned_to_me"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "1")
                    .set_body_json(serde_json::json!([mock_gitlab_issue(1, "Mine")])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let (issues, total) = client
            .fetch_project_issues(&params("", Some("opened"), Some("@me")), 20, 1)
            .unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].title, "Mine");
        assert_eq!(total, Some(1));
    }

    #[tokio::test]
    async fn test_search_issues_maps_me_to_assigned_to_me_scope() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("search", "bug"))
            .and(query_param("scope", "assigned_to_me"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "3")
                    .set_body_json(serde_json::json!([mock_gitlab_issue(5, "My bug")])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let (issues, total) = client
            .fetch_project_issues(&params("bug", Some("opened"), Some("me")), 20, 1)
            .unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(total, Some(3));
    }

    #[tokio::test]
    async fn test_count_issues_maps_me_to_assigned_to_me_scope() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("search", "bug"))
            .and(query_param("scope", "assigned_to_me"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "3")
                    .set_body_json(serde_json::json!([])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let (_, count) = client
            .fetch_project_issues(&params("bug", Some("opened"), Some("@me")), 1, 1)
            .unwrap();
        assert_eq!(count, Some(3));
    }

    #[tokio::test]
    async fn test_list_issues_passes_assignee_username() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("assignee_username[]", "someuser"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "1")
                    .set_body_json(serde_json::json!([mock_gitlab_issue(7, "Theirs")])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let (issues, total) = client
            .fetch_project_issues(&params("", None, Some("@someuser")), 20, 1)
            .unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].title, "Theirs");
        assert_eq!(total, Some(1));
    }

    #[tokio::test]
    async fn test_search_empty_query_sends_labels_order_by_and_sort() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("state", "opened"))
            .and(query_param("labels", "bug"))
            .and(query_param("order_by", "updated_at"))
            .and(query_param("sort", "desc"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "1")
                    .set_body_json(serde_json::json!([mock_gitlab_issue(9, "Bug")])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = <GitLabClient as IssueTracker>::search_issues(
            &client,
            "state=opened&labels=bug&order_by=updated_at&sort=desc",
            20,
            0,
        )
        .unwrap();

        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].summary, "Bug");
        assert_eq!(result.total, Some(1));
    }

    #[tokio::test]
    async fn test_count_honors_labels() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("labels", "priority::high"))
            .and(query_param("state", "opened"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "4")
                    .set_body_json(serde_json::json!([])),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let count = <GitLabClient as IssueTracker>::get_issue_count(
            &client,
            "state=opened&labels=priority::high",
        )
        .unwrap();
        assert_eq!(count, Some(4));
    }

    #[tokio::test]
    async fn test_issue_count_returns_none_when_header_absent() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("per_page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let (_, total) = client
            .fetch_project_issues(&GitLabQueryParams::default(), 1, 1)
            .unwrap();

        assert_eq!(total, None);
    }

    // ==================== Work Item Parent Operations ====================

    #[tokio::test]
    async fn test_set_work_item_parent() {
        let mock_server = MockServer::start().await;

        // The client base_url is the mock URI (no /api/v4 suffix),
        // so graphql_url() falls back to "{base_url}/graphql"
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "workItemUpdate": {
                        "workItem": {
                            "id": "gid://gitlab/Issue/1042"
                        },
                        "errors": []
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = client.set_work_item_parent(1042, 1010);

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_work_item_parent_graphql_top_level_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "errors": [
                    { "message": "You don't have permission to perform this action" }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = client.set_work_item_parent(1042, 1010);

        assert!(result.is_err());
        match result.unwrap_err() {
            TrackerError::Api { message, .. } => {
                assert!(
                    message.contains("GraphQL error"),
                    "Expected GraphQL error, got: {}",
                    message
                );
            }
            other => panic!("Expected Api error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_set_work_item_parent_mutation_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "workItemUpdate": {
                        "workItem": null,
                        "errors": ["Issue not found"]
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = client.set_work_item_parent(1042, 9999);

        assert!(result.is_err());
        match result.unwrap_err() {
            TrackerError::Api { message, .. } => {
                assert!(
                    message.contains("Failed to set parent"),
                    "Expected mutation error, got: {}",
                    message
                );
            }
            other => panic!("Expected Api error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_delete_issue_link() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/projects/123/issues/42/links/789"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = client.delete_issue_link(42, 789);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_issue_link_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/projects/123/issues/42/links/999"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "404 Not found"
            })))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = client.delete_issue_link(42, 999);
        assert!(result.is_err());
    }

    // ==================== Link Type Resolution Tests ====================

    #[test]
    fn test_link_type_defaults() {
        let client = GitLabClient::new("https://gitlab.com/api/v4", "test-token", Some("123"));

        assert_eq!(client.link_types.resolve("relates"), "relates_to");
        assert_eq!(client.link_types.resolve("depends"), "blocks");
        assert_eq!(client.link_types.resolve("required"), "is_blocked_by");
        assert_eq!(client.link_types.resolve("duplicates"), "relates_to");
        assert_eq!(client.link_types.resolve("duplicated-by"), "relates_to");
    }

    #[tokio::test]
    async fn test_list_articles_slices_full_wiki_response_past_first_hundred() {
        let mock_server = MockServer::start().await;

        // GitLab's GET /projects/:id/wikis does not support pagination --
        // `per_page`/`page` aren't documented parameters and are ignored;
        // the endpoint always returns the full collection in one response
        // (confirmed against the live API). `with_content` is the only
        // supported query param, so the mock must not require per_page/page.
        let wiki_pages: Vec<_> = (1..=105)
            .map(|n| mock_wiki_page(&format!("wiki-{n}"), &format!("Page {n}"), "body"))
            .collect();

        Mock::given(method("GET"))
            .and(path("/projects/123/wikis"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("with_content", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(wiki_pages))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        // skip=99 starts past the first 100 items that a (nonexistent) 100-
        // item page cap would have returned.
        let articles = client.list_articles(Some("123"), 5, 99).unwrap();

        assert_eq!(articles.len(), 5);
        assert_eq!(articles[0].id, "wiki-100");
        assert_eq!(articles[4].id, "wiki-104");
    }

    #[tokio::test]
    async fn test_list_all_articles_returns_articles_past_first_hundred() {
        let mock_server = MockServer::start().await;

        // Every call returns the same full 105-item collection (GitLab's
        // wiki-list endpoint has no pagination); `list_all_articles`'s
        // default windowed implementation calls `list_articles` twice
        // (offset 0 and offset 100) and must combine them without dropping
        // or duplicating anything.
        let wiki_pages: Vec<_> = (1..=105)
            .map(|n| mock_wiki_page(&format!("wiki-{n}"), &format!("Page {n}"), "body"))
            .collect();

        Mock::given(method("GET"))
            .and(path("/projects/123/wikis"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("with_content", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(wiki_pages))
            .expect(2)
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let articles = client.list_all_articles(Some("123"), 1000).unwrap();

        assert_eq!(articles.len(), 105);
        assert_eq!(articles[0].id, "wiki-1");
        // The 101st-105th wiki pages only surface once the second windowed
        // call slices past offset 100 -- exactly what silently dropping the
        // rest of the collection would have missed.
        assert_eq!(articles[100].id, "wiki-101");
        assert_eq!(articles[104].id, "wiki-105");
    }

    #[tokio::test]
    async fn test_search_all_articles_finds_match_past_first_hundred_wiki_pages() {
        let mock_server = MockServer::start().await;

        // The only match ("needle") sits at the end of a 105-item
        // collection returned in a single response.
        let mut wiki_pages: Vec<_> = (1..=104)
            .map(|n| mock_wiki_page(&format!("wiki-{n}"), &format!("Page {n}"), "haystack"))
            .collect();
        wiki_pages.push(mock_wiki_page(
            "wiki-105",
            "Needle Page",
            "contains the needle",
        ));

        Mock::given(method("GET"))
            .and(path("/projects/123/wikis"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("with_content", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(wiki_pages))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let articles = client.search_all_articles("needle", 1000).unwrap();

        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].id, "wiki-105");
    }

    // ==================== Issue History (3-endpoint merge) Tests ====================

    #[tokio::test]
    async fn test_get_issue_history_merges_three_endpoints() {
        let mock_server = MockServer::start().await;

        // resource_state_events: opened -> closed (out of order to exercise the
        // chronological sort + from-derivation).
        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42/resource_state_events"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 200,
                    "user": { "id": 2, "username": "bob", "name": "Bob" },
                    "created_at": "2024-01-05T00:00:00.000Z",
                    "state": "closed"
                },
                {
                    "id": 100,
                    "user": { "id": 1, "username": "alice", "name": "Alice" },
                    "created_at": "2024-01-01T00:00:00.000Z",
                    "state": "opened"
                }
            ])))
            .mount(&mock_server)
            .await;

        // resource_label_events: add bug.
        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42/resource_label_events"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 300,
                    "user": { "id": 3, "username": "carol", "name": "Carol" },
                    "created_at": "2024-01-03T00:00:00.000Z",
                    "action": "add",
                    "label": { "id": 9, "name": "bug" }
                }
            ])))
            .mount(&mock_server)
            .await;

        // resource_milestone_events: add v1.0.
        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42/resource_milestone_events"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 400,
                    "user": { "id": 4, "username": "dave", "name": "Dave" },
                    "created_at": "2024-01-04T00:00:00.000Z",
                    "action": "add",
                    "milestone": { "id": 1, "iid": 1, "title": "v1.0" }
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let events = <GitLabClient as IssueTracker>::get_issue_history(&client, "42").unwrap();

        // 2 state + 1 label + 1 milestone, merged and sorted newest-first.
        assert_eq!(events.len(), 4);

        // 01-05 closed (status, from-derived as "opened" -> "closed").
        assert_eq!(events[0].field, "status");
        assert_eq!(events[0].from.as_deref(), Some("opened"));
        assert_eq!(events[0].to.as_deref(), Some("closed"));
        assert_eq!(events[0].author.as_ref().unwrap().login, "bob");

        // 01-04 milestone add.
        assert_eq!(events[1].field, "milestone");
        assert_eq!(events[1].from, None);
        assert_eq!(events[1].to.as_deref(), Some("v1.0"));

        // 01-03 label add.
        assert_eq!(events[2].field, "labels");
        assert_eq!(events[2].from, None);
        assert_eq!(events[2].to.as_deref(), Some("bug"));

        // 01-01 opened (seed transition).
        assert_eq!(events[3].field, "status");
        assert_eq!(events[3].from.as_deref(), Some("opened"));
        assert_eq!(events[3].to.as_deref(), Some("opened"));
        assert_eq!(events[3].author.as_ref().unwrap().login, "alice");
    }

    // ==================== Full-collection pagination ====================

    /// Mount a project-scoped collection whose first page is full (so the
    /// client must ask for page 2) and whose second page holds `second_page`.
    /// The full first page answers any request without `page=2`, so a client
    /// that never paginates still gets it.
    async fn mount_two_page_collection(
        mock_server: &MockServer,
        collection_path: &str,
        first_page: Vec<serde_json::Value>,
        second_page: Vec<serde_json::Value>,
    ) {
        Mock::given(method("GET"))
            .and(path(collection_path))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(second_page))
            .with_priority(1)
            .mount(mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path(collection_path))
            .respond_with(ResponseTemplate::new(200).set_body_json(first_page))
            .mount(mock_server)
            .await;
    }

    /// A project with more than 100 labels: a label on page 2 must still be
    /// found by name, or `tags delete`/`tags update` report "not found".
    #[tokio::test]
    async fn trait_delete_tag_finds_label_past_first_page() {
        let mock_server = MockServer::start().await;
        let first_page = (1..=100)
            .map(|id| serde_json::json!({"id": id, "name": format!("label-{id}"), "color": "#ededed"}))
            .collect();
        let second_page =
            vec![serde_json::json!({"id": 101, "name": "target", "color": "#ededed"})];
        mount_two_page_collection(
            &mock_server,
            "/projects/123/labels",
            first_page,
            second_page,
        )
        .await;
        Mock::given(method("DELETE"))
            .and(path("/projects/123/labels/101"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let deleted = <GitLabClient as IssueTracker>::delete_tag(&client, "target");

        assert!(
            deleted.is_ok(),
            "label on page 2 must be found: {deleted:?}"
        );
    }

    #[tokio::test]
    async fn trait_list_project_users_returns_members_past_first_page() {
        let mock_server = MockServer::start().await;
        let member = |id: u64| serde_json::json!({"id": id, "username": format!("user{id}"), "name": "User"});
        mount_two_page_collection(
            &mock_server,
            "/projects/123/members/all",
            (1..=100).map(member).collect(),
            vec![member(101)],
        )
        .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let users = <GitLabClient as IssueTracker>::list_project_users(&client, "123").unwrap();

        assert_eq!(users.len(), 101);
        assert_eq!(users[100].login.as_deref(), Some("user101"));
    }

    // ==================== Parent resolution ====================

    /// A parent that cannot be resolved must fail before the issue is written:
    /// creating first would leave an orphaned issue, updating first would
    /// leave the issue half-updated.
    #[tokio::test]
    async fn trait_unresolvable_parent_writes_nothing() {
        for (case, parent) in [("missing", "#7"), ("malformed", "not-a-number")] {
            let mock_server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/projects/123/issues/7"))
                .respond_with(
                    ResponseTemplate::new(404)
                        .set_body_json(serde_json::json!({"message": "404 Not found"})),
                )
                .mount(&mock_server)
                .await;
            let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));

            let created = <GitLabClient as IssueTracker>::create_issue(
                &client,
                &CreateIssue {
                    project_id: "123".to_string(),
                    summary: "Child".to_string(),
                    description: None,
                    custom_fields: vec![],
                    tags: vec![],
                    parent: Some(parent.to_string()),
                },
            );
            let updated = <GitLabClient as IssueTracker>::update_issue(
                &client,
                "42",
                &UpdateIssue {
                    summary: Some("Renamed".to_string()),
                    parent: Some(parent.to_string()),
                    ..Default::default()
                },
            );

            assert!(created.is_err(), "{case} parent must fail create");
            assert!(updated.is_err(), "{case} parent must fail update");
            let requests = mock_server.received_requests().await.unwrap();
            assert!(
                requests.iter().all(|r| r.method.as_str() == "GET"),
                "{case} parent must not write anything, got {:?}",
                requests
                    .iter()
                    .map(|r| format!("{} {}", r.method, r.url.path()))
                    .collect::<Vec<_>>()
            );
        }
    }

    // ==================== Wiki hierarchy ====================

    #[tokio::test]
    async fn test_get_child_articles_returns_direct_children_only() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/projects/123/wikis"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                mock_wiki_page("docs", "docs", "root"),
                mock_wiki_page("docs/setup", "setup", "child"),
                mock_wiki_page("docs/setup/linux", "linux", "grandchild"),
                mock_wiki_page("docs-old/notes", "notes", "unrelated"),
            ])))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let children = client.get_child_articles("docs").unwrap();

        let ids: Vec<_> = children.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["docs/setup"]);
        assert_eq!(
            children[0].parent_article.as_ref().map(|p| p.id.as_str()),
            Some("docs")
        );
    }

    #[test]
    fn web_url_builds_project_path_urls_without_the_api() {
        let client = GitLabClient::new(
            "https://gitlab.example.com/api/v4",
            "test-token",
            Some("group/sub/project"),
        );

        assert_eq!(
            client.web_url(Some("#7")).unwrap().as_deref(),
            Some("https://gitlab.example.com/group/sub/project/-/issues/7")
        );
        assert_eq!(
            client.web_url(None).unwrap().as_deref(),
            Some("https://gitlab.example.com/group/sub/project/-/issues")
        );
    }

    #[tokio::test]
    async fn web_url_looks_up_numeric_project_ids() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/projects/123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 123,
                "name": "project",
                "web_url": "https://gitlab.example.com/group/project"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;
        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));

        assert_eq!(
            client.web_url(Some("7")).unwrap().as_deref(),
            Some("https://gitlab.example.com/group/project/-/issues/7")
        );
    }

    #[tokio::test]
    async fn web_url_rejects_a_malformed_iid_before_any_request() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock_server)
            .await;
        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));

        assert!(matches!(
            client.web_url(Some("PROJ-7")),
            Err(tracker_core::TrackerError::InvalidInput(_))
        ));
    }

    // ==================== Error and HTTP Plumbing ====================
    //
    // These pin how failures surface, through the IssueTracker trait wherever
    // it reaches the path.

    fn upload_file(path: PathBuf, mime_type: Option<&str>) -> AttachmentUploadFile {
        AttachmentUploadFile {
            path,
            name: None,
            mime_type: mime_type.map(str::to_string),
        }
    }

    fn upload_of(path: PathBuf, mime_type: Option<&str>) -> AttachmentUpload {
        AttachmentUpload {
            files: vec![upload_file(path, mime_type)],
            comment: None,
            silent: false,
            minor_edit: false,
        }
    }

    fn create_input(custom_fields: Vec<CustomFieldUpdate>) -> CreateIssue {
        CreateIssue {
            project_id: "123".to_string(),
            summary: "New issue".to_string(),
            description: None,
            custom_fields,
            tags: vec![],
            parent: None,
        }
    }

    fn assignee(login: &str) -> CustomFieldUpdate {
        CustomFieldUpdate::SingleUser {
            name: "Assignee".to_string(),
            login: login.to_string(),
        }
    }

    /// Unwrap a `TrackerError::Api` into its status and message.
    #[track_caller]
    fn api_error(err: TrackerError) -> (u16, String) {
        match err {
            TrackerError::Api { status, message } => (status, message),
            other => panic!("Expected Api error, got: {other:?}"),
        }
    }

    /// Serve one canned raw HTTP response, then close the connection. Used for
    /// responses wiremock cannot produce, such as a body cut off mid-stream.
    fn serve_raw_response_once(response: &'static str) -> String {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
                let mut buffer = [0; 4096];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn test_trait_get_issue_non_json_success_body_is_parse() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::get_issue(&client, "42").unwrap_err();

        // A 2xx body that is not the expected JSON is Parse; a failed read
        // stays Http.
        assert!(
            matches!(err, TrackerError::Parse(_)),
            "expected Parse, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_trait_search_non_json_success_body_is_parse() {
        let mock_server = MockServer::start().await;

        // The issue list is decoded from the raw response after its X-Total
        // header is read, a separate decode site from the GET helper.
        Mock::given(method("GET"))
            .and(path("/projects/123/issues"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-total", "1")
                    .set_body_string("not-json"),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::search_issues(&client, "", 20, 0).unwrap_err();

        // Parse, as for get_issue.
        assert!(
            matches!(err, TrackerError::Parse(_)),
            "expected Parse, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_trait_write_non_json_success_bodies_are_parse() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");

        for (http_method, write_path, status) in [
            ("POST", "/projects/123/issues", 201),
            ("PUT", "/projects/123/issues/42", 200),
            ("POST", "/projects/123/uploads", 201),
            ("POST", "/graphql", 200),
        ] {
            Mock::given(method(http_method))
                .and(path(write_path))
                .respond_with(ResponseTemplate::new(status).set_body_string("not-json"))
                .mount(&mock_server)
                .await;
        }

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let update = UpdateIssue {
            summary: Some("Renamed".to_string()),
            ..Default::default()
        };
        let results = [
            (
                "create (POST)",
                <GitLabClient as IssueTracker>::create_issue(&client, &create_input(vec![]))
                    .unwrap_err(),
            ),
            (
                "update (PUT)",
                <GitLabClient as IssueTracker>::update_issue(&client, "42", &update).unwrap_err(),
            ),
            (
                "upload (multipart)",
                <GitLabClient as IssueTracker>::add_issue_attachment(
                    &client,
                    "42",
                    &upload_of(upload_path.clone(), None),
                )
                .unwrap_err(),
            ),
            (
                "GraphQL",
                client.set_work_item_parent(1042, 1010).unwrap_err(),
            ),
        ];
        let _ = std::fs::remove_file(upload_path);

        // POST, PUT, multipart and GraphQL responses decode the same way as GET.
        let not_parse: Vec<_> = results
            .iter()
            .filter(|(_, err)| !matches!(err, TrackerError::Parse(_)))
            .collect();
        assert!(
            not_parse.is_empty(),
            "expected Parse from every call, got: {not_parse:?}"
        );
    }

    #[tokio::test]
    async fn test_trait_empty_success_body_on_json_path_is_parse() {
        let mock_server = MockServer::start().await;

        // GitLab's PUT answers with the issue; an empty 2xx body is not JSON.
        Mock::given(method("PUT"))
            .and(path("/projects/123/issues/42"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let update = UpdateIssue {
            summary: Some("Renamed".to_string()),
            ..Default::default()
        };
        let err = <GitLabClient as IssueTracker>::update_issue(&client, "42", &update).unwrap_err();

        // An empty body is not JSON either, so it is Parse too.
        assert!(
            matches!(err, TrackerError::Parse(_)),
            "expected Parse, got: {err:?}"
        );
    }

    #[test]
    fn test_truncated_chunked_success_body_is_http_not_parse() {
        // The connection drops mid-body. The body is read in full before
        // decoding, so the failed read is Http, not Parse.
        let base_url = serve_raw_response_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
             Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
             40\r\n{\"id\":1042,\"iid\":42,\"title\":\"Cut",
        );

        let client = GitLabClient::new(&base_url, "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::get_issue(&client, "42").unwrap_err();

        assert!(
            matches!(err, TrackerError::Http(_)),
            "expected Http, got: {err:?}"
        );
    }

    #[test]
    fn test_truncated_success_body_with_content_length_is_http_not_parse() {
        let base_url = serve_raw_response_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
             Content-Length: 100\r\nConnection: close\r\n\r\n\
             {\"id\":1042,\"iid\":42,\"title\":\"Cut",
        );

        let client = GitLabClient::new(&base_url, "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::get_issue(&client, "42").unwrap_err();

        assert!(
            matches!(err, TrackerError::Http(_)),
            "expected Http, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_oversized_success_body_is_http_not_parse() {
        let mock_server = MockServer::start().await;

        // Both bodies are over ureq's 10 MB read limit. The first is valid
        // JSON, so only the limit can fail it; the second is not JSON at all,
        // and still counts as a failed read rather than bad JSON.
        let mut huge_issue = mock_gitlab_issue(1, "Huge");
        huge_issue["description"] = serde_json::Value::String("x".repeat(11 * 1024 * 1024));
        Mock::given(method("GET"))
            .and(path("/projects/123/issues/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(huge_issue))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/projects/123/issues/2"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("x".repeat(10 * 1024 * 1024 + 1)),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        for id in ["1", "2"] {
            let err = <GitLabClient as IssueTracker>::get_issue(&client, id).unwrap_err();
            assert!(
                matches!(err, TrackerError::Http(_)),
                "issue {id}: expected Http, got: {err:?}"
            );
        }
    }

    #[test]
    fn test_transport_failure_is_http() {
        // Bind and drop a listener so nothing accepts on its port.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let client = GitLabClient::new(&format!("http://{addr}"), "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::get_issue(&client, "42").unwrap_err();

        assert!(
            matches!(err, TrackerError::Http(_)),
            "expected Http, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_error_message_key_order() {
        let mock_server = MockServer::start().await;

        // "message", then "error", then "error_description", whatever their
        // order in the body; a key counts only when its value is a string.
        let cases = [
            (1, r#"{"error":"e","message":"m"}"#, "m"),
            (2, r#"{"error_description":"d","error":"e"}"#, "e"),
            (3, r#"{"error_description":"d"}"#, "d"),
            (4, r#"{"message":["m"],"error":"e"}"#, "e"),
        ];
        for (iid, body, _) in cases {
            Mock::given(method("GET"))
                .and(path(format!("/projects/123/issues/{iid}")))
                .respond_with(ResponseTemplate::new(400).set_body_string(body))
                .mount(&mock_server)
                .await;
        }

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        for (iid, body, expected) in cases {
            let err =
                <GitLabClient as IssueTracker>::get_issue(&client, &iid.to_string()).unwrap_err();
            let (status, message) = api_error(err);
            assert_eq!(status, 400, "status for body {body}");
            assert_eq!(message, expected, "message for body {body}");
        }
    }

    #[tokio::test]
    async fn test_non_string_message_uses_raw_body() {
        let mock_server = MockServer::start().await;
        let body = r#"{"message":{"title":["can't be blank"]}}"#;

        // GitLab's validation errors put an object under "message"; with no
        // string to extract, the whole body is the message.
        Mock::given(method("POST"))
            .and(path("/projects/123/issues"))
            .respond_with(ResponseTemplate::new(400).set_body_string(body))
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::create_issue(&client, &create_input(vec![]))
            .unwrap_err();

        assert_eq!(api_error(err), (400, body.to_string()));
    }

    #[tokio::test]
    async fn test_error_body_without_string_message_is_raw_body_or_http_status() {
        let mock_server = MockServer::start().await;
        let cases = [
            (1, 400, "[]", "[]"),
            (2, 400, r#"{"foo":"bar"}"#, r#"{"foo":"bar"}"#),
            (3, 400, r#"{"message":null}"#, r#"{"message":null}"#),
            (4, 400, r#""Bad Request""#, r#""Bad Request""#),
            (
                5,
                502,
                "<html>Bad Gateway</html>",
                "<html>Bad Gateway</html>",
            ),
            (6, 500, "", "HTTP 500"),
        ];
        for (iid, status, body, _) in cases {
            Mock::given(method("GET"))
                .and(path(format!("/projects/123/issues/{iid}")))
                .respond_with(ResponseTemplate::new(status).set_body_string(body))
                .mount(&mock_server)
                .await;
        }

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        for (iid, expected_status, body, expected) in cases {
            let err =
                <GitLabClient as IssueTracker>::get_issue(&client, &iid.to_string()).unwrap_err();
            assert_eq!(
                api_error(err),
                (expected_status, expected.to_string()),
                "for body {body:?}"
            );
        }
    }

    #[test]
    fn test_error_body_read_failure_is_swallowed_as_http_status() {
        // The error body is cut short of its Content-Length. The read failure
        // is dropped and the message falls back to "HTTP <status>".
        let base_url = serve_raw_response_once(
            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\n\
             Content-Length: 100\r\nConnection: close\r\n\r\n\
             {\"message\":\"Bad Req",
        );

        let client = GitLabClient::new(&base_url, "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::get_issue(&client, "42").unwrap_err();

        assert_eq!(api_error(err), (400, "HTTP 400".to_string()));
    }

    #[tokio::test]
    async fn test_trait_get_issue_404_is_api_404() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({"message": "404 Not found"})),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::get_issue(&client, "42").unwrap_err();

        // Not NotFound or IssueNotFound: `track doctor` reads Api 404 itself.
        assert_eq!(api_error(err), (404, "404 Not found".to_string()));
    }

    #[tokio::test]
    async fn test_trait_status_mapping_only_401_is_special() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues/1"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(serde_json::json!({"message": "401 Unauthorized"})),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        // No NotFound mapping, no rate-limit detection, no retry: each of these
        // is a plain Api error after exactly one request.
        let cases = [
            (2, 403, r#"{"message":"403 Forbidden"}"#, "403 Forbidden"),
            (3, 429, "Retry later", "Retry later"),
            (
                4,
                500,
                r#"{"message":"500 Internal Server Error"}"#,
                "500 Internal Server Error",
            ),
            (5, 503, "", "HTTP 503"),
        ];
        for (iid, status, body, _) in cases {
            Mock::given(method("GET"))
                .and(path(format!("/projects/123/issues/{iid}")))
                .respond_with(
                    ResponseTemplate::new(status)
                        .insert_header("Retry-After", "0")
                        .insert_header("RateLimit-Remaining", "0")
                        .set_body_string(body),
                )
                .expect(1)
                .mount(&mock_server)
                .await;
        }

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));

        // 401 drops the server's message.
        let err = <GitLabClient as IssueTracker>::get_issue(&client, "1").unwrap_err();
        assert!(
            matches!(err, TrackerError::Unauthorized),
            "expected Unauthorized, got: {err:?}"
        );

        for (iid, expected_status, _, expected) in cases {
            let err =
                <GitLabClient as IssueTracker>::get_issue(&client, &iid.to_string()).unwrap_err();
            assert_eq!(api_error(err), (expected_status, expected.to_string()));
        }
    }

    #[tokio::test]
    async fn test_trait_web_url_without_project_web_url_is_api_0() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": 123, "name": "project"})),
            )
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::web_url(&client, None).unwrap_err();

        assert_eq!(
            api_error(err),
            (0, "GitLab returned no web_url for project 123".to_string())
        );
    }

    #[tokio::test]
    async fn test_unknown_assignee_is_synthetic_api_404() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/members/all"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .and(query_param("username", "ghost"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(201))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::create_issue(
            &client,
            &create_input(vec![assignee("ghost")]),
        )
        .unwrap_err();

        // Both lookups answered 200; the 404 is made up by the client.
        assert_eq!(
            api_error(err),
            (404, "GitLab user 'ghost' not found".to_string())
        );
    }

    #[tokio::test]
    async fn test_numeric_assignee_failures_are_api_404() {
        let mock_server = MockServer::start().await;

        // GET /users/7 answers with a different user: a synthetic 404.
        Mock::given(method("GET"))
            .and(path("/users/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 8,
                "username": "bob",
                "name": "Bob"
            })))
            .mount(&mock_server)
            .await;
        // GET /users/999999 is a real 404, read through the usual error path.
        Mock::given(method("GET"))
            .and(path("/users/999999"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({"message": "404 User Not Found"})),
            )
            .mount(&mock_server)
            .await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        for (login, expected) in [
            (
                "7",
                "GitLab user id '7' resolved to a different user (id 8)",
            ),
            ("999999", "404 User Not Found"),
        ] {
            let update = UpdateIssue {
                custom_fields: vec![assignee(login)],
                ..Default::default()
            };
            let err =
                <GitLabClient as IssueTracker>::update_issue(&client, "42", &update).unwrap_err();
            assert_eq!(
                api_error(err),
                (404, expected.to_string()),
                "assignee {login}"
            );
        }
    }

    #[tokio::test]
    async fn test_upload_missing_file_is_io_error() {
        let mock_server = MockServer::start().await;

        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        let missing = std::env::temp_dir().join(format!(
            "track-gitlab-upload-missing-{}.txt",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let err = <GitLabClient as IssueTracker>::add_issue_attachment(
            &client,
            "42",
            &upload_of(missing, None),
        )
        .unwrap_err();

        assert!(
            matches!(err, TrackerError::Io(_)),
            "expected Io, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_upload_invalid_mime_type_is_http_error() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("mime.txt", b"body");

        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let result = <GitLabClient as IssueTracker>::add_issue_attachment(
            &client,
            "42",
            &upload_of(upload_path.clone(), Some("not a mime")),
        );
        let _ = std::fs::remove_file(upload_path);

        // An input error, but ureq reports it, so it is Http.
        match result.unwrap_err() {
            TrackerError::Http(message) => assert!(
                message.starts_with("invalid MIME type"),
                "unexpected message: {message}"
            ),
            other => panic!("Expected Http error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_upload_names_and_types_part_from_path() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("notes.txt", b"notes");
        let file_name = upload_path.file_name().unwrap().to_str().unwrap();

        Mock::given(method("POST"))
            .and(path("/projects/123/uploads"))
            .and(body_string_contains("name=\"file\""))
            .and(body_string_contains(format!("filename=\"{file_name}\"")))
            .and(body_string_contains("Content-Type: text/plain"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "alt": "notes.txt",
                "url": "/uploads/notes.txt",
                "markdown": "[notes.txt](/uploads/notes.txt)"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let uploaded = client.upload_project_file(&upload_file(upload_path.clone(), None));
        let _ = std::fs::remove_file(&upload_path);

        assert_eq!(uploaded.unwrap().url, "/uploads/notes.txt");
    }

    #[tokio::test]
    async fn test_every_request_sends_private_token_and_accept_json() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("headers.txt", b"headers");
        let note = serde_json::json!({
            "id": 501,
            "body": "hi",
            "author": {"id": 1, "username": "testuser", "name": "Test User"},
            "created_at": "2024-01-15T14:00:00.000Z",
            "updated_at": "2024-01-15T14:00:00.000Z",
            "system": false
        });
        let upload = serde_json::json!({
            "alt": "headers.txt",
            "url": "/uploads/headers.txt",
            "markdown": "[headers.txt](/uploads/headers.txt)"
        });
        let graphql = serde_json::json!({"data": {"workItemUpdate": {"errors": []}}});

        let requests = [
            (
                "GET",
                "/projects/123/issues/42",
                None,
                200,
                Some(mock_gitlab_issue(42, "Got")),
            ),
            (
                "POST",
                "/projects/123/issues/42/notes",
                Some("application/json"),
                201,
                Some(note),
            ),
            (
                "PUT",
                "/projects/123/issues/42",
                Some("application/json"),
                200,
                Some(mock_gitlab_issue(42, "Put")),
            ),
            ("DELETE", "/projects/123/issues/42", None, 204, None),
            ("POST", "/projects/123/uploads", None, 201, Some(upload)),
            (
                "POST",
                "/graphql",
                Some("application/json"),
                200,
                Some(graphql),
            ),
        ];
        for (http_method, request_path, content_type, status, body) in requests {
            let mut mock = Mock::given(method(http_method))
                .and(path(request_path))
                .and(header("PRIVATE-TOKEN", "test-token"))
                .and(header("Accept", "application/json"));
            if let Some(content_type) = content_type {
                mock = mock.and(header("Content-Type", content_type));
            }
            let mut response = ResponseTemplate::new(status);
            if let Some(body) = body {
                response = response.set_body_json(body);
            }
            mock.respond_with(response)
                .expect(1)
                .mount(&mock_server)
                .await;
        }

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));
        let update = crate::models::UpdateGitLabIssue {
            title: Some("Put".to_string()),
            ..Default::default()
        };
        client.get_issue(42).unwrap();
        client.add_note(42, "hi").unwrap();
        client.update_issue(42, &update).unwrap();
        client.delete_issue(42).unwrap();
        let uploaded = client.upload_project_file(&upload_file(upload_path.clone(), None));
        let _ = std::fs::remove_file(&upload_path);
        uploaded.unwrap();
        client.set_work_item_parent(1042, 1010).unwrap();

        // GitLab auth is the PRIVATE-TOKEN header alone; no Authorization.
        let received = mock_server.received_requests().await.unwrap();
        assert_eq!(received.len(), 6);
        assert!(
            received
                .iter()
                .all(|request| !request.headers.contains_key("authorization")),
            "no request may carry an Authorization header"
        );
    }

    #[tokio::test]
    async fn test_redirect_is_followed_with_private_token() {
        let origin = MockServer::start().await;
        let target = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/123/issues/42"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/moved/42", target.uri()).as_str()),
            )
            .expect(1)
            .mount(&origin)
            .await;
        Mock::given(method("GET"))
            .and(path("/moved/42"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .and(header("Accept", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_gitlab_issue(42, "Moved")))
            .expect(1)
            .mount(&target)
            .await;

        let client = GitLabClient::new(&origin.uri(), "test-token", Some("123"));
        let issue = <GitLabClient as IssueTracker>::get_issue(&client, "42").unwrap();

        // ureq follows the redirect and strips only Authorization and Cookie,
        // so the custom PRIVATE-TOKEN header goes on to another origin too.
        // Pinned so that any change to it is deliberate.
        assert_eq!(issue.summary, "Moved");
    }

    #[tokio::test]
    async fn test_graphql_url_replaces_api_v4_suffix() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .and(header("PRIVATE-TOKEN", "test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {"workItemUpdate": {"errors": []}}
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        // The trailing slash is trimmed first, so the `/api/v4` suffix matches.
        let base_url = format!("{}/api/v4/", mock_server.uri());
        let client = GitLabClient::new(&base_url, "test-token", Some("123"));

        client.set_work_item_parent(1042, 1010).unwrap();
    }

    #[tokio::test]
    async fn test_graphql_errors_are_api_status_zero() {
        let mock_server = MockServer::start().await;

        // Each case has its own base path, so its endpoint is `{base}/graphql`.
        let responses = [
            (
                "/top",
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "errors": [
                        {"message": "first"},
                        {"extensions": {"code": "no message"}},
                        {"message": "second"}
                    ]
                })),
            ),
            (
                "/mutation",
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": {
                        "workItemUpdate": {"workItem": null, "errors": ["first", "second"]}
                    }
                })),
            ),
            (
                "/null",
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": null})),
            ),
            (
                "/status",
                ResponseTemplate::new(500).set_body_string(r#"{"errors":[{"message":"boom"}]}"#),
            ),
        ];
        for (base, response) in responses {
            Mock::given(method("POST"))
                .and(path(format!("{base}/graphql")))
                .respond_with(response)
                .expect(1)
                .mount(&mock_server)
                .await;
        }

        let set_parent = |base: &str| {
            let client = GitLabClient::new(
                &format!("{}{base}", mock_server.uri()),
                "test-token",
                Some("123"),
            );
            client.set_work_item_parent(1042, 1010)
        };

        // Errors in a 200 body are status 0; entries without a string message
        // are dropped from the joined text.
        assert_eq!(
            api_error(set_parent("/top").unwrap_err()),
            (0, "GraphQL error: first; second".to_string())
        );
        assert_eq!(
            api_error(set_parent("/mutation").unwrap_err()),
            (0, "Failed to set parent: first; second".to_string())
        );
        // Neither error list present: success, even with no data.
        set_parent("/null").unwrap();
        // A non-2xx GraphQL response goes through the REST error path, where
        // an `errors` body has no message key and is kept verbatim.
        assert_eq!(
            api_error(set_parent("/status").unwrap_err()),
            (500, r#"{"errors":[{"message":"boom"}]}"#.to_string())
        );
    }

    #[tokio::test]
    async fn test_delete_and_link_ignore_success_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/projects/123/issues/42"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .expect(1)
            .mount(&mock_server)
            .await;
        // The canonical "relates" must reach GitLab as its native link type.
        Mock::given(method("POST"))
            .and(path("/projects/123/issues/42/links"))
            .and(body_partial_json(
                serde_json::json!({"link_type": "relates_to"}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_string("not-json"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = GitLabClient::new(&mock_server.uri(), "test-token", Some("123"));

        // Neither path decodes its 2xx body, so a body that is not JSON is not
        // an error.
        <GitLabClient as IssueTracker>::delete_issue(&client, "42").unwrap();
        <GitLabClient as IssueTracker>::link_issues(&client, "42", "43", "relates", "outward")
            .unwrap();
    }
}
