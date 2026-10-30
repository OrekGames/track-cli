//! Unit tests for JiraClient using wiremock

#[cfg(test)]
mod tests {
    use crate::client::JiraClient;
    use crate::models::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tracker_core::{AttachmentUpload, AttachmentUploadFile};
    use wiremock::matchers::{
        any, body_string_contains, header, method, path, query_param, query_param_is_missing,
    };
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn temp_upload_file(name: &str, contents: &[u8]) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("track-jira-upload-{nanos}-{name}"));
        std::fs::write(&path, contents).unwrap();
        path
    }

    async fn setup_mock_client(mock_server: &MockServer) -> JiraClient {
        JiraClient::new(&mock_server.uri(), "test@example.com", "dummy_token")
    }

    #[tokio::test]
    async fn test_resolve_transition_id_greek_destination() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transitions": [
                    {
                        "id": "31",
                        "name": "Close",
                        "to": {
                            "id": "done",
                            "name": "Κλειστός"
                        }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        let id = client
            .resolve_transition_id("TEST-123", "  ΚΛΕΙΣΤΌΣ  ")
            .unwrap();
        assert_eq!(id, "31");
    }

    #[tokio::test]
    async fn test_resolve_transition_id_greek_name_fallback() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transitions": [
                    {
                        "id": "31",
                        "name": "Κλειστός",
                        "to": {
                            "id": "done",
                            "name": "Done"
                        }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        let id = client
            .resolve_transition_id("TEST-123", "  ΚΛΕΙΣΤΌΣ  ")
            .unwrap();
        assert_eq!(id, "31");
    }

    #[tokio::test]
    async fn test_resolve_transition_id_greek_negative_equality() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transitions": [
                    {
                        "id": "31",
                        "name": "Close",
                        "to": {
                            "id": "done",
                            "name": "Κλειστόσ"
                        }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::TrackerError;
        let err = client
            .resolve_transition_id("TEST-123", "ΚΛΕΙΣΤΌΣ")
            .unwrap_err();
        match err {
            TrackerError::InvalidInput(message) => assert_eq!(
                message,
                format!(
                    "status '{}' is not reachable. available: {:?}",
                    "ΚΛΕΙΣΤΌΣ",
                    vec!["Κλειστόσ"]
                )
            ),
            other => panic!("Expected InvalidInput error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_resolve_transition_id_destination_precedence() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transitions": [
                    {
                        "id": "10",
                        "name": "Done",
                        "to": {
                            "id": "open",
                            "name": "Open"
                        }
                    },
                    {
                        "id": "31",
                        "name": "Close",
                        "to": {
                            "id": "done",
                            "name": "Done"
                        }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        let id = client.resolve_transition_id("TEST-123", " done ").unwrap();
        assert_eq!(id, "31");
    }

    #[tokio::test]
    async fn test_resolve_transition_id_various_cases() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transitions": [
                    {
                        "id": "1",
                        "name": "Geöffnet",
                        "to": { "id": "1", "name": "Geöffnet" }
                    },
                    {
                        "id": "2",
                        "name": "À faire",
                        "to": { "id": "2", "name": "À faire" }
                    },
                    {
                        "id": "3",
                        "name": "Открыто",
                        "to": { "id": "3", "name": "Открыто" }
                    },
                    {
                        "id": "4",
                        "name": "i\u{307}",
                        "to": { "id": "4", "name": "i\u{307}" }
                    },
                    {
                        "id": "5",
                        "name": "K",
                        "to": { "id": "5", "name": "K" }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;

        assert_eq!(
            client
                .resolve_transition_id("TEST-123", "GEÖFFNET")
                .unwrap(),
            "1"
        );
        assert_eq!(
            client.resolve_transition_id("TEST-123", "À FAIRE").unwrap(),
            "2"
        );
        assert_eq!(
            client.resolve_transition_id("TEST-123", "ОТКРЫТО").unwrap(),
            "3"
        );
        assert_eq!(client.resolve_transition_id("TEST-123", "İ").unwrap(), "4");
        assert_eq!(client.resolve_transition_id("TEST-123", "K").unwrap(), "5");

        use tracker_core::TrackerError;
        let err = client
            .resolve_transition_id("TEST-123", "UNKNOWN")
            .unwrap_err();
        match err {
            TrackerError::InvalidInput(message) => assert_eq!(
                message,
                format!(
                    "status '{}' is not reachable. available: {:?}",
                    "UNKNOWN",
                    vec!["Geöffnet", "À faire", "Открыто", "i\u{307}", "K"]
                )
            ),
            other => panic!("Expected InvalidInput error, got: {other:?}"),
        }
    }

    fn base64_encode_for_test(input: &str) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        let bytes = input.as_bytes();
        let mut result = String::new();

        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
            let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

            result.push(ALPHABET[b0 >> 2] as char);
            result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

            if chunk.len() > 1 {
                result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
            } else {
                result.push('=');
            }

            if chunk.len() > 2 {
                result.push(ALPHABET[b2 & 0x3f] as char);
            } else {
                result.push('=');
            }
        }

        result
    }

    /// Helper to create a mock Jira issue response
    fn mock_jira_issue(key: &str, summary: &str) -> serde_json::Value {
        mock_jira_issue_with_id("10001", key, summary)
    }

    /// Like `mock_jira_issue`, but with a distinct internal id — needed by
    /// tests that exercise dedup-by-id pagination.
    fn mock_jira_issue_with_id(id: &str, key: &str, summary: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "key": key,
            "self": format!("https://test.atlassian.net/rest/api/3/issue/{}", key),
            "fields": {
                "summary": summary,
                "description": {
                    "type": "doc",
                    "version": 1,
                    "content": [
                        {
                            "type": "paragraph",
                            "content": [
                                {
                                    "type": "text",
                                    "text": "Test description"
                                }
                            ]
                        }
                    ]
                },
                "status": {
                    "id": "1",
                    "name": "Open",
                    "statusCategory": {
                        "key": "new",
                        "name": "To Do"
                    }
                },
                "priority": {
                    "id": "3",
                    "name": "Medium"
                },
                "issuetype": {
                    "id": "10001",
                    "name": "Task",
                    "subtask": false
                },
                "project": {
                    "id": "10000",
                    "key": "TEST",
                    "name": "Test Project"
                },
                "assignee": null,
                "reporter": {
                    "accountId": "123456",
                    "displayName": "Test User",
                    "emailAddress": "test@example.com",
                    "active": true
                },
                "labels": ["bug", "urgent"],
                "components": [
                    {
                        "self": "https://test.atlassian.net/rest/api/3/component/10001",
                        "id": "10001",
                        "name": "Rendering",
                        "description": "Rendering subsystem"
                    },
                    {
                        "self": "https://test.atlassian.net/rest/api/3/component/10002",
                        "id": "10002",
                        "name": "Audio"
                    }
                ],
                "created": "2024-01-15T10:30:00.000+0000",
                "updated": "2024-01-15T12:00:00.000+0000",
                "subtasks": [],
                "issuelinks": []
            }
        })
    }

    /// Helper to create a mock Jira project response
    fn mock_jira_project(key: &str, name: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "10000",
            "key": key,
            "name": name,
            "self": format!("https://test.atlassian.net/rest/api/3/project/{}", key),
            "projectTypeKey": "software"
        })
    }

    #[tokio::test]
    async fn test_auth_whitespace_trimming() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123"))
            // Base64 for "test@test.com:FAKE-TOKEN-DO-NOT-USE" is "dGVzdEB0ZXN0LmNvbTpGQUtFLVRPS0VOLURPLU5PVC1VU0U="
            // The client should trim the whitespace before encoding
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTpGQUtFLVRPS0VOLURPLU5PVC1VU0U=",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_jira_issue("TEST-123", "Test issue")),
            )
            .mount(&mock_server)
            .await;

        // Pass credentials with leading/trailing whitespace
        let client = JiraClient::new(
            &mock_server.uri(),
            " test@test.com \n",
            "\rFAKE-TOKEN-DO-NOT-USE\t ",
        );
        let issue = client.get_issue("TEST-123").unwrap();

        assert_eq!(issue.key, "TEST-123");
    }

    #[tokio::test]
    async fn test_auth_whitespace_proof_dirty_header_differs_but_client_sends_clean_header() {
        let mock_server = MockServer::start().await;
        let dirty_email = " test@test.com \n";
        let dirty_token = "\rFAKE-TOKEN-DO-NOT-USE\t ";
        let clean_credentials = "test@test.com:FAKE-TOKEN-DO-NOT-USE";
        let dirty_credentials = format!("{}:{}", dirty_email, dirty_token);
        let clean_header = format!("Basic {}", base64_encode_for_test(clean_credentials));
        let dirty_header = format!("Basic {}", base64_encode_for_test(&dirty_credentials));

        assert_ne!(
            dirty_header, clean_header,
            "Untrimmed credentials should produce a different Basic auth header"
        );

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .and(header("Authorization", clean_header.as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_jira_issue("TEST-123", "Test issue")),
            )
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), dirty_email, dirty_token);
        let issue = client.get_issue("TEST-123").unwrap();

        assert_eq!(issue.key, "TEST-123");
    }

    #[tokio::test]
    async fn test_get_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_jira_issue("TEST-123", "Test issue")),
            )
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let issue = client.get_issue("TEST-123").unwrap();

        assert_eq!(issue.key, "TEST-123");
        assert_eq!(issue.fields.summary, "Test issue");
        assert_eq!(issue.fields.status.name, "Open");
        assert_eq!(issue.fields.labels, vec!["bug", "urgent"]);

        // Components must be deserialized into the named field (pulled out of
        // the flattened `extra` catch-all), not silently dropped.
        let component_names: Vec<&str> = issue
            .fields
            .components
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(component_names, vec!["Rendering", "Audio"]);
        assert!(!issue.fields.extra.contains_key("components"));
    }

    #[tokio::test]
    async fn test_add_issue_attachment_uses_file_field_and_xsrf_header() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");

        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/TEST-123/attachments"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .and(header("X-Atlassian-Token", "no-check"))
            .and(body_string_contains("name=\"file\""))
            .and(body_string_contains("filename=\"custom.txt\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "10001",
                    "filename": "custom.txt",
                    "size": 8,
                    "mimeType": "text/plain",
                    "content": "https://test.atlassian.net/attachment/content/10001",
                    "author": {
                        "accountId": "abc123",
                        "displayName": "Test User",
                        "emailAddress": "test@example.com",
                        "active": true
                    }
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let upload = AttachmentUpload {
            files: vec![AttachmentUploadFile {
                path: upload_path.clone(),
                name: Some("custom.txt".to_string()),
                mime_type: Some("text/plain".to_string()),
            }],
            comment: None,
            silent: false,
            minor_edit: false,
        };

        let attachments = client
            .add_issue_attachments("TEST-123", &upload)
            .expect("issue attachment upload should succeed");

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].filename, "custom.txt");
        tokio::task::spawn_blocking(move || std::fs::remove_file(upload_path))
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_search_issues() {
        let mock_server = MockServer::start().await;

        // The new Jira API uses GET /search/jql with cursor-based pagination
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .and(query_param("maxResults", "20"))
            .and(query_param_is_missing("startAt"))
            .and(query_param_is_missing("nextPageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-123", "First issue"),
                    mock_jira_issue("TEST-124", "Second issue")
                ],
                "isLast": true
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let result = client.search_issues("project = TEST", 20, None).unwrap();

        assert!(result.is_last);
        assert!(result.next_page_token.is_none());
        assert_eq!(result.issues.len(), 2);
        assert_eq!(result.issues[0].key, "TEST-123");
        assert_eq!(result.issues[1].key, "TEST-124");
    }

    #[tokio::test]
    async fn test_create_issue() {
        let mock_server = MockServer::start().await;

        // First mock: create issue returns minimal response
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": "10002",
                "key": "TEST-125",
                "self": "https://test.atlassian.net/rest/api/3/issue/TEST-125"
            })))
            .mount(&mock_server)
            .await;

        // Second mock: get issue to fetch full details
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-125"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_jira_issue("TEST-125", "New issue")),
            )
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let create = CreateJiraIssue {
            fields: CreateJiraIssueFields {
                project: ProjectId {
                    id: None,
                    key: Some("TEST".to_string()),
                },
                summary: "New issue".to_string(),
                description: Some(markdown_to_adf("New description")),
                issuetype: IssueTypeId {
                    id: None,
                    name: Some("Task".to_string()),
                },
                priority: None,
                labels: None,
                parent: None,
                extra: std::collections::HashMap::new(),
            },
        };

        let issue = client.create_issue(&create).unwrap();
        assert_eq!(issue.key, "TEST-125");
        assert_eq!(issue.fields.summary, "New issue");
    }

    #[tokio::test]
    async fn test_update_issue() {
        let mock_server = MockServer::start().await;

        // Update returns 204 No Content; the client does not re-fetch
        Mock::given(method("PUT"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let update = UpdateJiraIssue {
            fields: UpdateJiraIssueFields {
                summary: Some("Updated summary".to_string()),
                description: None,
                priority: None,
                labels: None,
                parent: None,
                assignee: None,
                extra: std::collections::HashMap::new(),
            },
        };

        client.update_issue("TEST-123", &update).unwrap();
    }

    #[tokio::test]
    async fn test_update_issue_with_time_tracking() {
        let mock_server = MockServer::start().await;

        // Update returns 204 No Content and asserts body format
        Mock::given(method("PUT"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "fields": {
                    "timetracking": {
                        "originalEstimate": "4h"
                    }
                }
            })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        let mut extra = std::collections::HashMap::new();
        extra.insert(
            "timetracking".to_string(),
            serde_json::json!({
                "originalEstimate": "4h"
            }),
        );

        let update = UpdateJiraIssue {
            fields: UpdateJiraIssueFields {
                summary: None,
                description: None,
                priority: None,
                labels: None,
                parent: None,
                assignee: None,
                extra,
            },
        };

        client.update_issue("TEST-123", &update).unwrap();
    }

    #[tokio::test]
    async fn test_delete_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let result = client.delete_issue("TEST-123");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_projects() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/project"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                mock_jira_project("TEST", "Test Project"),
                mock_jira_project("DEMO", "Demo Project")
            ])))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let projects = client.list_projects().unwrap();

        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].key, "TEST");
        assert_eq!(projects[0].name, "Test Project");
        assert_eq!(projects[1].key, "DEMO");
    }

    #[tokio::test]
    async fn test_get_project() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/project/TEST"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_jira_project("TEST", "Test Project")),
            )
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let project = client.get_project("TEST").unwrap();

        assert_eq!(project.key, "TEST");
        assert_eq!(project.name, "Test Project");
    }

    #[tokio::test]
    async fn test_add_comment() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/TEST-123/comment"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": "10001",
                "self": "https://test.atlassian.net/rest/api/3/issue/TEST-123/comment/10001",
                "body": {
                    "type": "doc",
                    "version": 1,
                    "content": [
                        {
                            "type": "paragraph",
                            "content": [
                                {
                                    "type": "text",
                                    "text": "This is a comment"
                                }
                            ]
                        }
                    ]
                },
                "author": {
                    "accountId": "123456",
                    "displayName": "Test User",
                    "emailAddress": "test@example.com",
                    "active": true
                },
                "created": "2024-01-15T14:00:00.000+0000",
                "updated": "2024-01-15T14:00:00.000+0000"
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let comment = client.add_comment("TEST-123", "This is a comment").unwrap();

        assert_eq!(comment.id, "10001");
        assert!(comment.author.is_some());
    }

    #[tokio::test]
    async fn test_get_comments() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/comment"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "startAt": 0,
                "maxResults": 50,
                "total": 2,
                "comments": [
                    {
                        "id": "10001",
                        "body": {
                            "type": "doc",
                            "version": 1,
                            "content": [
                                {
                                    "type": "paragraph",
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": "First comment"
                                        }
                                    ]
                                }
                            ]
                        },
                        "author": {
                            "accountId": "123456",
                            "displayName": "User One"
                        },
                        "created": "2024-01-15T10:00:00.000+0000",
                        "updated": "2024-01-15T10:00:00.000+0000"
                    },
                    {
                        "id": "10002",
                        "body": {
                            "type": "doc",
                            "version": 1,
                            "content": [
                                {
                                    "type": "paragraph",
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": "Second comment"
                                        }
                                    ]
                                }
                            ]
                        },
                        "author": {
                            "accountId": "789012",
                            "displayName": "User Two"
                        },
                        "created": "2024-01-15T11:00:00.000+0000",
                        "updated": "2024-01-15T11:00:00.000+0000"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let comments = client.get_comments("TEST-123").unwrap();

        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].id, "10001");
        assert_eq!(comments[1].id, "10002");
    }

    #[tokio::test]
    async fn test_get_comments_page_uses_pagination_params() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/comment"))
            .and(query_param("startAt", "20"))
            .and(query_param("maxResults", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "startAt": 20,
                "maxResults": 10,
                "total": 20,
                "comments": []
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let comments = client.get_comments_page("TEST-123", 10, 20).unwrap();

        assert!(comments.is_empty());
    }

    #[tokio::test]
    async fn test_get_issue_history_paginates_and_sorts() {
        use tracker_core::IssueTracker;

        let mock_server = MockServer::start().await;

        // Page 1: two entries, not the last page.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/changelog"))
            .and(query_param("startAt", "0"))
            .and(query_param("maxResults", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "startAt": 0,
                "maxResults": 2,
                "total": 3,
                "isLast": false,
                "values": [
                    {
                        "id": "1",
                        "author": { "accountId": "acc-1", "displayName": "Alice" },
                        "created": "2024-01-10T09:00:00.000+0000",
                        "items": [
                            { "field": "status", "fromString": "To Do", "toString": "In Progress" }
                        ]
                    },
                    {
                        "id": "2",
                        "author": { "accountId": "acc-2", "displayName": "Bob" },
                        "created": "2024-01-12T11:00:00.000+0000",
                        "items": [
                            { "field": "assignee", "fromString": null, "toString": "Alice" }
                        ]
                    }
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Page 2: the final entry (newest). isLast stays false, so only
        // reaching `total` (3) ends the walk; a third request would 404.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/changelog"))
            .and(query_param("startAt", "2"))
            .and(query_param("maxResults", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "startAt": 2,
                "maxResults": 2,
                "total": 3,
                "isLast": false,
                "values": [
                    {
                        "id": "3",
                        "author": { "accountId": "acc-1", "displayName": "Alice" },
                        "created": "2024-01-15T14:00:00.000+0000",
                        "items": [
                            { "field": "status", "fromString": "In Progress", "toString": "Done" }
                        ]
                    }
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let events = client.get_issue_history("TEST-123").unwrap();

        // Three entries across two pages, one item each => three events.
        assert_eq!(events.len(), 3);
        // Newest-first ordering.
        assert_eq!(events[0].field, "status");
        assert_eq!(events[0].to.as_deref(), Some("Done"));
        assert_eq!(events[2].to.as_deref(), Some("In Progress"));
        // Author mapping (accountId -> login, displayName -> name).
        assert_eq!(
            events[0].author.as_ref().map(|a| a.login.as_str()),
            Some("acc-1")
        );
        // Null fromString maps to None.
        let assignee = events.iter().find(|e| e.field == "assignee").unwrap();
        assert_eq!(assignee.from, None);
    }

    #[tokio::test]
    async fn test_unauthorized_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "errorMessages": ["You do not have permission to view this issue."],
                "errors": {}
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "bad-token");
        let result = client.get_issue("TEST-123");

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            tracker_core::TrackerError::Unauthorized
        ));
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/NONEXISTENT-999"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "errorMessages": ["Issue does not exist or you do not have permission to see it."],
                "errors": {}
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let err = client.get_issue("NONEXISTENT-999").unwrap_err();

        // Unlike Confluence, Jira keeps the server's 404 text. Neither maps
        // 404 to a not-found variant.
        assert_eq!(
            expect_api(err),
            (
                404,
                "Issue does not exist or you do not have permission to see it.".to_string()
            )
        );
    }

    #[tokio::test]
    async fn test_adf_text_extraction() {
        // Test that we can extract text from ADF format
        let adf = serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "Hello "
                        },
                        {
                            "type": "text",
                            "text": "World"
                        }
                    ]
                }
            ]
        });

        let text = adf_to_text(&adf);
        assert_eq!(text, "Hello World");
    }

    #[tokio::test]
    async fn test_adf_text_extraction_multiline() {
        // Test that block nodes like paragraphs and list items preserve newlines and bullets
        let adf = serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "First paragraph"
                        }
                    ]
                },
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "Second paragraph"
                        }
                    ]
                },
                {
                    "type": "bulletList",
                    "content": [
                        {
                            "type": "listItem",
                            "content": [
                                {
                                    "type": "text",
                                    "text": "Item A"
                                }
                            ]
                        },
                        {
                            "type": "listItem",
                            "content": [
                                {
                                    "type": "text",
                                    "text": "Item B"
                                }
                            ]
                        }
                    ]
                }
            ]
        });

        let text = adf_to_text(&adf);
        assert_eq!(
            text,
            "First paragraph\n\nSecond paragraph\n\n- Item A\n- Item B"
        );
    }

    #[tokio::test]
    async fn test_markdown_to_adf_conversion() {
        // Test that plain text produces a valid ADF doc
        let adf = markdown_to_adf("Hello World");

        assert_eq!(adf["type"], "doc");
        assert_eq!(adf["version"], 1);
        assert!(adf["content"].is_array());
    }

    #[tokio::test]
    async fn test_search_with_pagination() {
        let mock_server = MockServer::start().await;

        // Client-level token round-trip: the first page hands back a token,
        // and the next request must carry it (and never a startAt offset).
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param_is_missing("nextPageToken"))
            .and(query_param_is_missing("startAt"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-11", "Issue 11"),
                    mock_jira_issue("TEST-12", "Issue 12")
                ],
                "nextPageToken": "tok-1",
                "isLast": false
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", "tok-1"))
            .and(query_param_is_missing("startAt"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-13", "Issue 13")
                ],
                "isLast": true
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        let page1 = client.search_issues("project = TEST", 2, None).unwrap();
        assert_eq!(page1.next_page_token.as_deref(), Some("tok-1"));
        assert_eq!(page1.issues.len(), 2);

        let page2 = client
            .search_issues("project = TEST", 2, page1.next_page_token.as_deref())
            .unwrap();
        assert!(page2.next_page_token.is_none());
        assert!(page2.is_last);
        assert_eq!(page2.issues[0].key, "TEST-13");
    }

    #[tokio::test]
    async fn test_trait_search_walks_token_chain() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("maxResults", "4"))
            .and(query_param_is_missing("nextPageToken"))
            .and(query_param_is_missing("startAt"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-1", "Issue 1"),
                    mock_jira_issue("TEST-2", "Issue 2")
                ],
                "nextPageToken": "t2"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("maxResults", "2"))
            .and(query_param("nextPageToken", "t2"))
            .and(query_param_is_missing("startAt"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-3", "Issue 3"),
                    mock_jira_issue("TEST-4", "Issue 4")
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::IssueTracker;
        let result = IssueTracker::search_issues(&client, "project = TEST", 4, 0).unwrap();

        let keys: Vec<&str> = result
            .items
            .iter()
            .map(|i| i.id_readable.as_str())
            .collect();
        assert_eq!(keys, vec!["TEST-1", "TEST-2", "TEST-3", "TEST-4"]);
    }

    #[tokio::test]
    async fn test_trait_search_skip_across_page_boundary() {
        let mock_server = MockServer::start().await;

        // skip=4, limit=2 over pages of 3+3: the entire first page is
        // discarded, the remaining skip of 1 applies within page 2.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param_is_missing("nextPageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-1", "Issue 1"),
                    mock_jira_issue("TEST-2", "Issue 2"),
                    mock_jira_issue("TEST-3", "Issue 3")
                ],
                "nextPageToken": "t2"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", "t2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-4", "Issue 4"),
                    mock_jira_issue("TEST-5", "Issue 5"),
                    mock_jira_issue("TEST-6", "Issue 6")
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::IssueTracker;
        let result = IssueTracker::search_issues(&client, "project = TEST", 2, 4).unwrap();

        let keys: Vec<&str> = result
            .items
            .iter()
            .map(|i| i.id_readable.as_str())
            .collect();
        assert_eq!(keys, vec!["TEST-5", "TEST-6"]);
    }

    #[tokio::test]
    async fn test_trait_search_terminates_on_missing_token() {
        let mock_server = MockServer::start().await;

        // No nextPageToken and no isLast in the response (serde defaults):
        // token absence alone must terminate the walk after one request.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-1", "Issue 1"),
                    mock_jira_issue("TEST-2", "Issue 2"),
                    mock_jira_issue("TEST-3", "Issue 3")
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::IssueTracker;
        let result = IssueTracker::search_issues(&client, "project = TEST", 50, 0).unwrap();
        assert_eq!(result.items.len(), 3);
    }

    #[tokio::test]
    async fn test_trait_search_limit_over_100_accumulates() {
        let mock_server = MockServer::start().await;

        let page1: Vec<serde_json::Value> = (1..=100)
            .map(|i| mock_jira_issue_with_id(&i.to_string(), &format!("TEST-{i}"), "Issue"))
            .collect();
        let page2: Vec<serde_json::Value> = (101..=150)
            .map(|i| mock_jira_issue_with_id(&i.to_string(), &format!("TEST-{i}"), "Issue"))
            .collect();

        // Requests are clamped to the server's 100 cap, then shrink to the
        // remainder.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("maxResults", "100"))
            .and(query_param_is_missing("nextPageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": page1,
                "nextPageToken": "t2"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("maxResults", "50"))
            .and(query_param("nextPageToken", "t2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": page2
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::IssueTracker;
        let result = IssueTracker::search_issues(&client, "project = TEST", 150, 0).unwrap();
        assert_eq!(result.items.len(), 150);
        assert_eq!(result.items[0].id_readable, "TEST-1");
        assert_eq!(result.items[149].id_readable, "TEST-150");
    }

    #[tokio::test]
    async fn test_trait_search_errors_on_repeated_token() {
        let mock_server = MockServer::start().await;

        // A server that echoes the same token forever must not hang the
        // walk or silently emit duplicate rows: the second response repeats
        // the token just used, so the walk fails loudly after exactly two
        // requests.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [mock_jira_issue("TEST-1", "Issue 1")],
                "nextPageToken": "same"
            })))
            .expect(2)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::{IssueTracker, TrackerError};
        let result = IssueTracker::search_issues(&client, "project = TEST", 10, 0);
        assert!(matches!(result, Err(TrackerError::PaginationStalled(_))));
    }

    #[tokio::test]
    async fn test_trait_search_continues_past_empty_page_with_fresh_token() {
        let mock_server = MockServer::start().await;

        // An empty page with a fresh token is NOT the end of results
        // (token absence is the only authoritative last-page signal).
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param_is_missing("nextPageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-1", "Issue 1"),
                    mock_jira_issue("TEST-2", "Issue 2")
                ],
                "nextPageToken": "t2"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", "t2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [],
                "nextPageToken": "t3"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", "t3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-3", "Issue 3"),
                    mock_jira_issue("TEST-4", "Issue 4")
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::IssueTracker;
        let result = IssueTracker::search_issues(&client, "project = TEST", 4, 0).unwrap();

        let keys: Vec<&str> = result
            .items
            .iter()
            .map(|i| i.id_readable.as_str())
            .collect();
        assert_eq!(keys, vec!["TEST-1", "TEST-2", "TEST-3", "TEST-4"]);
    }

    #[tokio::test]
    async fn test_trait_search_skip_beyond_end_returns_empty() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue("TEST-1", "Issue 1"),
                    mock_jira_issue("TEST-2", "Issue 2"),
                    mock_jira_issue("TEST-3", "Issue 3")
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::IssueTracker;
        let result = IssueTracker::search_issues(&client, "project = TEST", 5, 10).unwrap();
        assert!(result.items.is_empty());
    }

    #[tokio::test]
    async fn test_trait_search_limit_zero_makes_no_requests() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": []
            })))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::IssueTracker;
        let result = IssueTracker::search_issues(&client, "project = TEST", 0, 0).unwrap();
        assert!(result.items.is_empty());
    }

    #[tokio::test]
    async fn test_search_issues_percent_encodes_token() {
        let mock_server = MockServer::start().await;

        // Tokens are opaque server strings; characters like '+', '/', '=',
        // '&' must survive the URL round-trip. wiremock decodes the query
        // string, so this matcher only matches if the client encoded it.
        let raw_token = "tok+1/x=&y";
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", raw_token))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [mock_jira_issue("TEST-1", "Issue 1")]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let result = client
            .search_issues("project = TEST", 5, Some(raw_token))
            .unwrap();
        assert_eq!(result.issues.len(), 1);
    }

    #[tokio::test]
    async fn test_search_all_issues_walks_token_chain_with_short_page() {
        let mock_server = MockServer::start().await;

        // Three pages; the middle one is deliberately short (1 issue) —
        // per the API contract a short page must NOT terminate the walk,
        // only token absence does.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param_is_missing("nextPageToken"))
            .and(query_param_is_missing("startAt"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue_with_id("1", "TEST-1", "Issue 1"),
                    mock_jira_issue_with_id("2", "TEST-2", "Issue 2")
                ],
                "nextPageToken": "t2"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", "t2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue_with_id("3", "TEST-3", "Issue 3")
                ],
                "nextPageToken": "t3"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", "t3"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue_with_id("4", "TEST-4", "Issue 4"),
                    mock_jira_issue_with_id("5", "TEST-5", "Issue 5")
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::IssueTracker;
        let result = IssueTracker::search_all_issues(&client, "project = TEST", 1000).unwrap();

        let keys: Vec<&str> = result.iter().map(|i| i.id_readable.as_str()).collect();
        assert_eq!(keys, vec!["TEST-1", "TEST-2", "TEST-3", "TEST-4", "TEST-5"]);
    }

    #[tokio::test]
    async fn test_search_all_issues_errors_on_repeated_token() {
        let mock_server = MockServer::start().await;

        // The second response echoes back the token that was just used:
        // the cursor is not advancing. Must fail loudly instead of looping.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param_is_missing("nextPageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue_with_id("1", "TEST-1", "Issue 1"),
                    mock_jira_issue_with_id("2", "TEST-2", "Issue 2")
                ],
                "nextPageToken": "t2"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("nextPageToken", "t2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue_with_id("1", "TEST-1", "Issue 1"),
                    mock_jira_issue_with_id("2", "TEST-2", "Issue 2")
                ],
                "nextPageToken": "t2"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::{IssueTracker, TrackerError};
        let result = IssueTracker::search_all_issues(&client, "project = TEST", 1000);
        assert!(matches!(result, Err(TrackerError::PaginationStalled(_))));
    }

    #[tokio::test]
    async fn test_search_all_issues_errors_after_no_progress_run() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        /// Always returns the same single issue but a fresh token on every
        /// request — the cursor "advances" without ever yielding new data.
        struct FreshTokenSamePage(AtomicUsize);
        impl wiremock::Respond for FreshTokenSamePage {
            fn respond(&self, _request: &wiremock::Request) -> ResponseTemplate {
                let n = self.0.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "issues": [mock_jira_issue_with_id("1", "TEST-1", "Issue 1")],
                    "nextPageToken": format!("t{}", n + 1)
                }))
            }
        }

        let mock_server = MockServer::start().await;

        // Request 1 makes progress (1 new issue); requests 2..=6 each yield
        // zero new issues with a fresh token, exhausting the no-progress
        // budget of 5 and failing loudly.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .respond_with(FreshTokenSamePage(AtomicUsize::new(0)))
            .expect(6)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::{IssueTracker, TrackerError};
        let result = IssueTracker::search_all_issues(&client, "project = TEST", 1000);
        assert!(matches!(result, Err(TrackerError::PaginationStalled(_))));
    }

    #[tokio::test]
    async fn test_search_all_issues_respects_max_results() {
        let mock_server = MockServer::start().await;

        // max_results = 3: the first request asks for 3, the second for the
        // remaining 1, and the walk stops without following the last token.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("maxResults", "3"))
            .and(query_param_is_missing("nextPageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue_with_id("1", "TEST-1", "Issue 1"),
                    mock_jira_issue_with_id("2", "TEST-2", "Issue 2")
                ],
                "nextPageToken": "t2"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(query_param("maxResults", "1"))
            .and(query_param("nextPageToken", "t2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issues": [
                    mock_jira_issue_with_id("3", "TEST-3", "Issue 3")
                ],
                "nextPageToken": "t3"
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::IssueTracker;
        let result = IssueTracker::search_all_issues(&client, "project = TEST", 3).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_issue_with_subtasks() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "10100",
                "key": "TEST-100",
                "self": "https://test.atlassian.net/rest/api/3/issue/TEST-100",
                "fields": {
                    "summary": "Parent issue",
                    "status": {
                        "id": "1",
                        "name": "Open",
                        "statusCategory": { "key": "new" }
                    },
                    "issuetype": {
                        "id": "10001",
                        "name": "Story",
                        "subtask": false
                    },
                    "project": {
                        "id": "10000",
                        "key": "TEST"
                    },
                    "labels": [],
                    "created": "2024-01-15T10:00:00.000+0000",
                    "updated": "2024-01-15T10:00:00.000+0000",
                    "subtasks": [
                        {
                            "id": "10101",
                            "key": "TEST-101",
                            "self": "https://test.atlassian.net/rest/api/3/issue/TEST-101",
                            "fields": {
                                "summary": "Subtask 1",
                                "status": { "name": "Open" },
                                "issuetype": { "name": "Sub-task" }
                            }
                        },
                        {
                            "id": "10102",
                            "key": "TEST-102",
                            "self": "https://test.atlassian.net/rest/api/3/issue/TEST-102",
                            "fields": {
                                "summary": "Subtask 2",
                                "status": { "name": "Done" },
                                "issuetype": { "name": "Sub-task" }
                            }
                        }
                    ],
                    "issuelinks": []
                }
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let issue = client.get_issue("TEST-100").unwrap();

        assert_eq!(issue.key, "TEST-100");
        assert_eq!(issue.fields.subtasks.len(), 2);
        assert_eq!(issue.fields.subtasks[0].key, "TEST-101");
        assert_eq!(issue.fields.subtasks[1].key, "TEST-102");
    }

    #[tokio::test]
    async fn test_list_labels() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/label"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total": 2,
                "maxResults": 1000,
                "values": ["bug", "enhancement"]
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let labels = client.list_labels().unwrap();

        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], "bug");
        assert_eq!(labels[1], "enhancement");
    }

    #[tokio::test]
    async fn test_list_labels_empty() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/label"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total": 0,
                "maxResults": 1000,
                "values": []
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let labels = client.list_labels().unwrap();

        assert_eq!(labels.len(), 0);
    }

    #[tokio::test]
    async fn test_issue_with_links() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-200"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "10200",
                "key": "TEST-200",
                "self": "https://test.atlassian.net/rest/api/3/issue/TEST-200",
                "fields": {
                    "summary": "Issue with links",
                    "status": {
                        "id": "1",
                        "name": "Open",
                        "statusCategory": { "key": "new" }
                    },
                    "issuetype": {
                        "id": "10001",
                        "name": "Task",
                        "subtask": false
                    },
                    "project": {
                        "id": "10000",
                        "key": "TEST"
                    },
                    "labels": [],
                    "created": "2024-01-15T10:00:00.000+0000",
                    "updated": "2024-01-15T10:00:00.000+0000",
                    "subtasks": [],
                    "issuelinks": [
                        {
                            "id": "20001",
                            "type": {
                                "id": "10000",
                                "name": "Blocks",
                                "inward": "is blocked by",
                                "outward": "blocks"
                            },
                            "outwardIssue": {
                                "id": "10201",
                                "key": "TEST-201",
                                "fields": {
                                    "summary": "Blocked issue"
                                }
                            }
                        },
                        {
                            "id": "20002",
                            "type": {
                                "id": "10001",
                                "name": "Relates",
                                "inward": "relates to",
                                "outward": "relates to"
                            },
                            "inwardIssue": {
                                "id": "10202",
                                "key": "TEST-202",
                                "fields": {
                                    "summary": "Related issue"
                                }
                            }
                        }
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let issue = client.get_issue("TEST-200").unwrap();

        assert_eq!(issue.key, "TEST-200");
        assert_eq!(issue.fields.issuelinks.len(), 2);
        assert_eq!(issue.fields.issuelinks[0].link_type.name, "Blocks");
        assert!(issue.fields.issuelinks[0].outward_issue.is_some());
        assert_eq!(issue.fields.issuelinks[1].link_type.name, "Relates");
        assert!(issue.fields.issuelinks[1].inward_issue.is_some());
    }

    #[tokio::test]
    async fn test_delete_link() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/rest/api/3/issueLink/12345"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let result = client.delete_link("12345");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_link_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/rest/api/3/issueLink/99999"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "errorMessages": ["No issue link with id '99999' exists."],
                "errors": {}
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let result = client.delete_link("99999");
        assert!(result.is_err());
    }

    // ==================== Link Type Resolution Tests ====================

    #[test]
    fn test_link_type_defaults() {
        let client = JiraClient::new("https://test.atlassian.net", "a@b.com", "tok");

        assert_eq!(client.link_types.resolve("relates"), "Relates");
        assert_eq!(client.link_types.resolve("depends"), "Blocks");
        assert_eq!(client.link_types.resolve("required"), "Blocks");
        assert_eq!(client.link_types.resolve("duplicates"), "Duplicate");
        assert_eq!(client.link_types.resolve("duplicated-by"), "Duplicate");
    }

    #[tokio::test]
    async fn test_get_issue_captures_custom_fields_in_extra() {
        let mock_server = MockServer::start().await;

        let mut issue_json = mock_jira_issue("TEST-500", "Issue with custom fields");
        // Add custom fields to the mock response
        let fields = issue_json
            .get_mut("fields")
            .unwrap()
            .as_object_mut()
            .unwrap();
        fields.insert("customfield_10016".to_string(), serde_json::json!(5.0));
        fields.insert(
            "customfield_10020".to_string(),
            serde_json::json!([{"id": 1, "name": "Sprint 1"}]),
        );
        fields.insert(
            "customfield_11000".to_string(),
            serde_json::json!({"value": "Option A"}),
        );
        fields.insert("customfield_11001".to_string(), serde_json::Value::Null);

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-500"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(issue_json))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let issue = client.get_issue("TEST-500").unwrap();

        // Standard fields still work
        assert_eq!(issue.key, "TEST-500");
        assert_eq!(issue.fields.summary, "Issue with custom fields");

        // Custom fields captured in extra HashMap
        assert_eq!(
            issue.fields.extra.get("customfield_10016").unwrap(),
            &serde_json::json!(5.0)
        );
        assert_eq!(
            issue.fields.extra.get("customfield_10020").unwrap(),
            &serde_json::json!([{"id": 1, "name": "Sprint 1"}])
        );
        assert_eq!(
            issue.fields.extra.get("customfield_11000").unwrap(),
            &serde_json::json!({"value": "Option A"})
        );
        // Null fields are still captured in the map (filtering happens during conversion)
        assert!(issue.fields.extra.contains_key("customfield_11001"));
    }

    #[tokio::test]
    async fn test_update_issue_with_state_calls_transitions_endpoint() {
        let mock_server = MockServer::start().await;

        // 1. Mock GET transitions
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transitions": [
                    {
                        "id": "31",
                        "name": "Start Progress",
                        "to": {
                            "id": "3",
                            "name": "In Progress"
                        }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        // 2. Mock POST transition
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/TEST-123/transitions"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "transition": { "id": "31" }
            })))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        // 3. Mock GET issue (re-fetch)
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_jira_issue("TEST-123", "Test issue")),
            )
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        use tracker_core::{CustomFieldUpdate, IssueTracker, UpdateIssue};
        let update = UpdateIssue {
            custom_fields: vec![CustomFieldUpdate::State {
                name: "Status".to_string(),
                value: "In Progress".to_string(),
            }],
            ..Default::default()
        };

        let result = IssueTracker::update_issue(&client, "TEST-123", &update);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_issue_with_state_and_summary_sends_both_requests() {
        let mock_server = MockServer::start().await;

        // 1. Mock PUT field update
        Mock::given(method("PUT"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "fields": {
                    "summary": "New Summary"
                }
            })))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        // 2. Mock GET transitions
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123/transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transitions": [
                    {
                        "id": "31",
                        "name": "Start Progress",
                        "to": { "id": "3", "name": "In Progress" }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        // 3. Mock POST transition
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/TEST-123/transitions"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "transition": { "id": "31" }
            })))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        // 4. Mock GET issue: one re-fetch after both writes, none in between
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(mock_jira_issue("TEST-123", "New Summary")),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        use tracker_core::{CustomFieldUpdate, IssueTracker, UpdateIssue};
        let update = UpdateIssue {
            summary: Some("New Summary".to_string()),
            custom_fields: vec![CustomFieldUpdate::State {
                name: "Status".to_string(),
                value: "In Progress".to_string(),
            }],
            ..Default::default()
        };

        let result = IssueTracker::update_issue(&client, "TEST-123", &update);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_link_issues_depends_creates_blocks_with_correct_direction() {
        let mock_server = MockServer::start().await;

        // Expect POST to /rest/api/3/issueLink with Blocks type and correct direction
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issueLink"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "type": { "name": "Blocks" },
                "inwardIssue": { "key": "PROJ-1" },
                "outwardIssue": { "key": "PROJ-2" }
            })))
            .respond_with(ResponseTemplate::new(201))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        // "depends" + "OUTWARD" means: PROJ-1 depends on PROJ-2
        // which is: PROJ-2 blocks PROJ-1
        // so outward=PROJ-2 (the blocker), inward=PROJ-1 (the blocked)
        use tracker_core::IssueTracker;
        let result = client.link_issues("PROJ-1", "PROJ-2", "depends", "OUTWARD");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_issue_rejects_conflicting_status_values() {
        // No mocks needed: the conflict is detected before any request is sent
        // (the /field metadata fetch degrades gracefully to an empty list).
        let mock_server = MockServer::start().await;
        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::{CustomFieldUpdate, IssueTracker, TrackerError, UpdateIssue};
        let update = UpdateIssue {
            custom_fields: vec![
                CustomFieldUpdate::State {
                    name: "State".to_string(),
                    value: "Done".to_string(),
                },
                CustomFieldUpdate::State {
                    name: "Status".to_string(),
                    value: "In Progress".to_string(),
                },
            ],
            ..Default::default()
        };

        let err = IssueTracker::update_issue(&client, "TEST-1", &update).unwrap_err();
        match err {
            TrackerError::InvalidInput(msg) => {
                assert!(
                    msg.contains("Done") && msg.contains("In Progress"),
                    "{}",
                    msg
                );
            }
            other => panic!("expected InvalidInput, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_update_issue_collapses_duplicate_status_values() {
        // Two State updates carrying the same value are not a conflict:
        // exactly one transition must fire.
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-1/transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "transitions": [
                    { "id": "31", "name": "Done", "to": { "id": "3", "name": "Done" } }
                ]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/TEST-1/transitions"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-1"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_jira_issue("TEST-1", "Test")),
            )
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");

        use tracker_core::{CustomFieldUpdate, IssueTracker, UpdateIssue};
        let update = UpdateIssue {
            custom_fields: vec![
                CustomFieldUpdate::State {
                    name: "State".to_string(),
                    value: "Done".to_string(),
                },
                CustomFieldUpdate::State {
                    name: "Status".to_string(),
                    value: "Done".to_string(),
                },
            ],
            ..Default::default()
        };

        let issue = IssueTracker::update_issue(&client, "TEST-1", &update).unwrap();
        assert_eq!(issue.id_readable, "TEST-1");
    }

    #[tokio::test]
    async fn test_get_fields_cached_recovers_from_poisoned_mutex() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/field"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "customfield_10000",
                    "name": "Owned test fixture",
                    "custom": true,
                    "schema": null
                }
            ])))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = std::sync::Arc::new(JiraClient::new(
            &mock_server.uri(),
            "test@test.com",
            "test-token",
        ));

        // Poison the lock by panicking on another thread while holding the lock
        let client_clone = std::sync::Arc::clone(&client);
        let handle = std::thread::spawn(move || {
            let _guard = client_clone.field_cache.lock().unwrap();
            panic!("Intentional panic to poison mutex");
        });
        assert!(handle.join().is_err());
        assert!(client.field_cache.is_poisoned());

        // Mutex is now poisoned; get_fields_cached should recover gracefully without panicking,
        // fetch fields via GET /rest/api/3/field, and return them.
        let fields = client.get_fields_cached();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].id, "customfield_10000");
        assert_eq!(fields[0].name, "Owned test fixture");

        // Subsequent call reuses cached fields without making another HTTP request.
        let cached_fields = client.get_fields_cached();
        assert_eq!(cached_fields.len(), 1);
        assert_eq!(cached_fields[0].id, "customfield_10000");
    }

    #[tokio::test]
    async fn test_get_fields_cached_recovers_from_poisoned_mutex_with_populated_cache() {
        let mock_server = MockServer::start().await;

        // HTTP requests should NOT be made since the cache is already seeded
        Mock::given(method("GET"))
            .and(path("/rest/api/3/field"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = std::sync::Arc::new(JiraClient::new(
            &mock_server.uri(),
            "test@test.com",
            "test-token",
        ));

        // Seed the cache before poisoning
        {
            let mut cache = client.field_cache.lock().unwrap();
            *cache = Some(vec![JiraField {
                id: "customfield_20000".to_string(),
                name: "Seeded field".to_string(),
                custom: true,
                schema: None,
            }]);
        }

        // Poison the lock by panicking on another thread while holding the lock
        let client_clone = std::sync::Arc::clone(&client);
        let handle = std::thread::spawn(move || {
            let _guard = client_clone.field_cache.lock().unwrap();
            panic!("Intentional panic to poison mutex");
        });
        assert!(handle.join().is_err());
        assert!(client.field_cache.is_poisoned());

        // Mutex is poisoned, but cached data is preserved and returned without HTTP call
        let fields = client.get_fields_cached();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].id, "customfield_20000");
        assert_eq!(fields[0].name, "Seeded field");
    }

    // ==================== Error and HTTP Plumbing Characterization ====================
    //
    // These pin the error classification and request shape that the shared
    // tracker-core HTTP helpers must keep. Errors are checked through the
    // IssueTracker trait wherever it reaches the path.

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

    fn minimal_create_issue() -> tracker_core::CreateIssue {
        tracker_core::CreateIssue {
            project_id: "X".to_string(),
            summary: "New issue".to_string(),
            description: None,
            custom_fields: vec![],
            tags: vec![],
            parent: None,
        }
    }

    fn summary_update(summary: &str) -> UpdateJiraIssue {
        UpdateJiraIssue {
            fields: UpdateJiraIssueFields {
                summary: Some(summary.to_string()),
                description: None,
                priority: None,
                labels: None,
                parent: None,
                assignee: None,
                extra: std::collections::HashMap::new(),
            },
        }
    }

    /// The status and message of an `Api` error; panics on any other variant.
    fn expect_api(err: tracker_core::TrackerError) -> (u16, String) {
        match err {
            tracker_core::TrackerError::Api { status, message } => (status, message),
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
            .and(path("/rest/api/3/issue/X-1"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::{IssueTracker, TrackerError};
        let err = IssueTracker::get_issue(&client, "X-1").unwrap_err();

        // A 2xx body that is not the expected JSON is Parse; a failed read
        // stays Http.
        assert!(
            matches!(err, TrackerError::Parse(_)),
            "expected Parse, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_trait_create_comment_and_upload_non_json_success_body_is_parse() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");

        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue"))
            .respond_with(ResponseTemplate::new(201).set_body_string("not-json"))
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/X-1/comment"))
            .respond_with(ResponseTemplate::new(201).set_body_string("not-json"))
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/X-1/attachments"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::{IssueTracker, TrackerError};
        let create_err = IssueTracker::create_issue(&client, &minimal_create_issue()).unwrap_err();
        let comment_err = IssueTracker::add_comment(&client, "X-1", "hello").unwrap_err();
        let upload = upload_of(upload_path.clone(), None);
        let upload_err = IssueTracker::add_issue_attachment(&client, "X-1", &upload).unwrap_err();
        let _ = std::fs::remove_file(upload_path);

        // POST and multipart responses decode the same way as GET.
        for err in [create_err, comment_err, upload_err] {
            assert!(
                matches!(err, TrackerError::Parse(_)),
                "expected Parse, got: {err:?}"
            );
        }
    }

    #[test]
    fn test_truncated_success_body_is_http_not_parse() {
        // The connection drops mid-body, chunked or with a Content-Length. The
        // body is read in full before decoding, so the failed read is Http.
        let responses = [
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
             Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
             40\r\n{\"id\":\"10001\",\"key\":\"X-1\"",
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
             Content-Length: 100\r\nConnection: close\r\n\r\n\
             {\"id\":\"10001\",\"key\":\"X-1\"",
        ];

        use tracker_core::{IssueTracker, TrackerError};
        for response in responses {
            let base_url = serve_raw_response_once(response);
            let client = JiraClient::new(&base_url, "test@test.com", "test-token");
            let err = IssueTracker::get_issue(&client, "X-1").unwrap_err();

            assert!(
                matches!(err, TrackerError::Http(_)),
                "expected Http, got: {err:?}"
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
             {\"errorMessages\":[\"Bad Req",
        );

        let client = JiraClient::new(&base_url, "test@test.com", "test-token");
        use tracker_core::IssueTracker;
        let err = IssueTracker::get_issue(&client, "X-1").unwrap_err();

        assert_eq!(expect_api(err), (400, "HTTP 400".to_string()));
    }

    #[tokio::test]
    async fn test_error_messages_and_field_errors_joined() {
        let mock_server = MockServer::start().await;

        // The errors{} keys are out of alphabetical order on purpose, and one
        // value is not a string.
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue"))
            .respond_with(ResponseTemplate::new(400).set_body_string(
                r#"{"errorMessages":["A"],"errors":{"summary":"S","assignee":"X","n":1}}"#,
            ))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::IssueTracker;
        let err = IssueTracker::create_issue(&client, &minimal_create_issue()).unwrap_err();

        // errorMessages first, then "field: msg" for string values only, in
        // alphabetical field order (serde_json is built without preserve_order).
        assert_eq!(
            expect_api(err),
            (400, "A; assignee: X; summary: S".to_string())
        );
    }

    #[tokio::test]
    async fn test_error_body_without_known_fields_uses_raw_body() {
        let mock_server = MockServer::start().await;
        let cases = [
            ("X-1", r#"{"errorMessages":[],"errors":{}}"#),
            // Jira ignores a top-level "message" (the Confluence client reads it).
            ("X-2", r#"{"message":"m"}"#),
        ];

        for (key, body) in cases {
            Mock::given(method("GET"))
                .and(path(format!("/rest/api/3/issue/{key}")))
                .respond_with(ResponseTemplate::new(400).set_body_string(body))
                .mount(&mock_server)
                .await;
        }

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::IssueTracker;
        for (key, body) in cases {
            let err = IssueTracker::get_issue(&client, key).unwrap_err();
            assert_eq!(expect_api(err), (400, body.to_string()), "{key}");
        }
    }

    #[tokio::test]
    async fn test_error_non_json_and_bare_json_bodies_fall_back() {
        let mock_server = MockServer::start().await;
        // Only an empty, non-JSON body becomes "HTTP <status>". Other text, and
        // JSON that is not an object, is passed through as the raw body.
        let cases = [
            ("X-1", 500, "", "HTTP 500"),
            ("X-2", 502, "Bad Gateway", "Bad Gateway"),
            ("X-3", 400, r#""Bad Request""#, r#""Bad Request""#),
            ("X-4", 400, "42", "42"),
            ("X-5", 400, "null", "null"),
        ];

        for (key, status, body, _) in cases {
            Mock::given(method("GET"))
                .and(path(format!("/rest/api/3/issue/{key}")))
                .respond_with(ResponseTemplate::new(status).set_body_string(body))
                .mount(&mock_server)
                .await;
        }

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::IssueTracker;
        for (key, status, _, message) in cases {
            let err = IssueTracker::get_issue(&client, key).unwrap_err();
            assert_eq!(expect_api(err), (status, message.to_string()), "{key}");
        }
    }

    #[tokio::test]
    async fn test_no_special_casing_or_retry_for_403_429_or_5xx() {
        let mock_server = MockServer::start().await;
        let cases = [("X-1", 403), ("X-2", 429), ("X-3", 503)];

        // Each is requested once: no retry, and Retry-After is not read.
        for (key, status) in cases {
            Mock::given(method("GET"))
                .and(path(format!("/rest/api/3/issue/{key}")))
                .respond_with(
                    ResponseTemplate::new(status)
                        .insert_header("Retry-After", "1")
                        .set_body_json(serde_json::json!({
                            "errorMessages": [format!("failed with {status}")],
                            "errors": {}
                        })),
                )
                .expect(1)
                .mount(&mock_server)
                .await;
        }

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::IssueTracker;
        for (key, status) in cases {
            let err = IssueTracker::get_issue(&client, key).unwrap_err();
            assert_eq!(
                expect_api(err),
                (status, format!("failed with {status}")),
                "{key}"
            );
        }
    }

    #[tokio::test]
    async fn test_attachment_comment_rejected_with_status_zero() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");

        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        let upload = AttachmentUpload {
            comment: Some("see attached".to_string()),
            ..upload_of(upload_path.clone(), None)
        };
        use tracker_core::IssueTracker;
        let err = IssueTracker::add_issue_attachment(&client, "X-1", &upload).unwrap_err();
        let _ = std::fs::remove_file(upload_path);

        // A sentinel status 0 Api error, not InvalidInput or Unsupported.
        assert_eq!(
            expect_api(err),
            (
                0,
                "Jira issue attachment upload does not support --comment".to_string()
            )
        );
    }

    #[tokio::test]
    async fn test_attachment_missing_file_is_io_and_bad_mime_is_http() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");
        let missing_path = upload_path.with_extension("missing");

        // Both fail while building the form, before any request.
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .expect(0)
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::{IssueTracker, TrackerError};
        let missing_err =
            IssueTracker::add_issue_attachment(&client, "X-1", &upload_of(missing_path, None))
                .unwrap_err();
        let mime_upload = upload_of(upload_path.clone(), Some("not a mime"));
        let mime_err =
            IssueTracker::add_issue_attachment(&client, "X-1", &mime_upload).unwrap_err();
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

    #[tokio::test]
    async fn test_attachment_ignores_silent_and_minor_edit() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");

        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue/X-1/attachments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        let upload = AttachmentUpload {
            silent: true,
            minor_edit: true,
            ..upload_of(upload_path.clone(), None)
        };
        use tracker_core::IssueTracker;
        IssueTracker::add_issue_attachment(&client, "X-1", &upload).unwrap();
        let _ = std::fs::remove_file(upload_path);

        // Only the file part is sent; the flags have no Jira form field.
        let requests = mock_server.received_requests().await.unwrap();
        let body = String::from_utf8_lossy(&requests[0].body);
        assert_eq!(body.matches("; name=\"").count(), 1, "{body}");
        assert!(body.contains("; name=\"file\""), "{body}");
    }

    #[tokio::test]
    async fn test_send_json_content_type_has_no_charset() {
        let mock_server = MockServer::start().await;

        // The explicit header stops ureq adding "application/json; charset=utf-8".
        Mock::given(method("PUT"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        client
            .update_issue("TEST-123", &summary_update("Updated summary"))
            .unwrap();
    }

    #[tokio::test]
    async fn test_requests_carry_auth_and_accept_json_headers() {
        let mock_server = MockServer::start().await;
        let upload_path = temp_upload_file("evidence.txt", b"evidence");
        let issue_path = "/rest/api/3/issue/TEST-123";

        // authorized() adds both headers to every request, multipart included.
        for (verb, route, response) in [
            (
                "GET",
                issue_path.to_string(),
                ResponseTemplate::new(200).set_body_json(mock_jira_issue("TEST-123", "Test")),
            ),
            ("PUT", issue_path.to_string(), ResponseTemplate::new(204)),
            ("DELETE", issue_path.to_string(), ResponseTemplate::new(204)),
            (
                "POST",
                format!("{issue_path}/attachments"),
                ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ),
        ] {
            Mock::given(method(verb))
                .and(path(route))
                .and(header(
                    "Authorization",
                    "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
                ))
                .and(header("Accept", "application/json"))
                .respond_with(response)
                .expect(1)
                .mount(&mock_server)
                .await;
        }

        // A trailing slash on the base URL is trimmed.
        let base_url = format!("{}/", mock_server.uri());
        let client = JiraClient::new(&base_url, "test@test.com", "test-token");
        client.get_issue("TEST-123").unwrap();
        client
            .update_issue("TEST-123", &summary_update("Updated summary"))
            .unwrap();
        client.delete_issue("TEST-123").unwrap();
        client
            .add_issue_attachments("TEST-123", &upload_of(upload_path.clone(), None))
            .unwrap();
        let _ = std::fs::remove_file(upload_path);
    }

    #[tokio::test]
    async fn test_body_discarding_calls_accept_non_json_success_body() {
        let mock_server = MockServer::start().await;

        // update, delete, link, unlink and transition never read the response
        // body, so a non-JSON 2xx still succeeds.
        for (verb, route, status) in [
            ("PUT", "/rest/api/3/issue/X-1", 200),
            ("DELETE", "/rest/api/3/issue/X-1", 200),
            ("POST", "/rest/api/3/issueLink", 201),
            ("DELETE", "/rest/api/3/issueLink/77", 200),
            ("POST", "/rest/api/3/issue/X-1/transitions", 200),
        ] {
            Mock::given(method(verb))
                .and(path(route))
                .respond_with(ResponseTemplate::new(status).set_body_string("not-json"))
                .expect(1)
                .mount(&mock_server)
                .await;
        }

        let client = setup_mock_client(&mock_server).await;
        client
            .update_issue("X-1", &summary_update("Updated summary"))
            .unwrap();
        client.delete_issue("X-1").unwrap();
        client
            .create_link(&CreateJiraIssueLink {
                link_type: IssueLinkTypeName {
                    name: "Blocks".to_string(),
                },
                inward_issue: IssueKeyRef {
                    key: "X-1".to_string(),
                },
                outward_issue: IssueKeyRef {
                    key: "X-2".to_string(),
                },
            })
            .unwrap();
        client.delete_link("77").unwrap();
        client.transition_issue("X-1", "31").unwrap();
    }

    #[tokio::test]
    async fn test_trait_resolve_project_id_masks_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/project/PROJ"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "errorMessages": ["Token expired"],
                "errors": {}
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::{IssueTracker, TrackerError};
        let err = IssueTracker::resolve_project_id(&client, "PROJ").unwrap_err();

        match err {
            TrackerError::ProjectNotFound(id) => assert_eq!(id, "PROJ"),
            other => panic!("Expected ProjectNotFound error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_trait_resolve_project_id_masks_server_and_network_errors() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/project/PROJ"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        // A port nothing listens on: the connection is refused.
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_url = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);

        use tracker_core::{IssueTracker, TrackerError};
        for base_url in [mock_server.uri(), closed_url] {
            let client = JiraClient::new(&base_url, "test@test.com", "test-token");
            let err = IssueTracker::resolve_project_id(&client, "PROJ").unwrap_err();
            match err {
                TrackerError::ProjectNotFound(id) => assert_eq!(id, "PROJ"),
                other => panic!("Expected ProjectNotFound error, got: {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn test_trait_project_custom_fields_ignore_status_and_component_errors() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/project/PROJ/statuses"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/project/PROJ/components"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;
        use tracker_core::IssueTracker;
        let fields = IssueTracker::get_project_custom_fields(&client, "PROJ").unwrap();

        // Both failures fall back to the hardcoded standard fields.
        let standard = crate::convert::get_standard_custom_fields();
        let summary = |fields: &[tracker_core::ProjectCustomField]| -> Vec<(String, Vec<String>)> {
            fields
                .iter()
                .map(|f| (f.id.clone(), f.values.clone()))
                .collect()
        };
        assert_eq!(summary(&fields), summary(&standard));
    }

    #[tokio::test]
    async fn test_get_fields_cached_swallows_errors_without_caching() {
        let mock_server = MockServer::start().await;

        // Mocks match in mount order, each up to its limit: a 500, then a body
        // of the wrong shape, then a real field list.
        Mock::given(method("GET"))
            .and(path("/rest/api/3/field"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/field"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "not": "a list"
            })))
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/field"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": "customfield_10000", "name": "Team", "custom": true }
            ])))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = setup_mock_client(&mock_server).await;

        // Failures degrade to an empty list and are not cached.
        assert!(client.get_fields_cached().is_empty());
        assert!(client.get_fields_cached().is_empty());
        assert_eq!(client.get_fields_cached().len(), 1);
        // A success is cached: no fourth request.
        assert_eq!(client.get_fields_cached().len(), 1);
    }
}
