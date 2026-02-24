//! Unit tests for JiraClient using wiremock

#[cfg(test)]
mod tests {
    use crate::client::JiraClient;
    use crate::models::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Helper to create a mock Jira issue response
    fn mock_jira_issue(key: &str, summary: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "10001",
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
    }

    #[tokio::test]
    async fn test_search_issues() {
        let mock_server = MockServer::start().await;

        // The new Jira API uses GET /search/jql with query parameters
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "startAt": 0,
                "maxResults": 20,
                "total": 2,
                "issues": [
                    mock_jira_issue("TEST-123", "First issue"),
                    mock_jira_issue("TEST-124", "Second issue")
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let result = client.search_issues("project = TEST", 20, 0).unwrap();

        assert_eq!(result.total, 2);
        assert_eq!(result.issues.len(), 2);
        assert_eq!(result.issues[0].key, "TEST-123");
        assert_eq!(result.issues[1].key, "TEST-124");
    }

    #[tokio::test]
    async fn test_count_issues() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .and(wiremock::matchers::query_param("maxResults", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "startAt": 0,
                "maxResults": 0,
                "total": 847,
                "issues": []
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let count = client.count_issues("project = TEST").unwrap();

        assert_eq!(count, 847);
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
                description: Some(text_to_adf("New description")),
                issuetype: IssueTypeId {
                    id: None,
                    name: Some("Task".to_string()),
                },
                priority: None,
                labels: None,
                parent: None,
            },
        };

        let issue = client.create_issue(&create).unwrap();
        assert_eq!(issue.key, "TEST-125");
        assert_eq!(issue.fields.summary, "New issue");
    }

    #[tokio::test]
    async fn test_update_issue() {
        let mock_server = MockServer::start().await;

        // First mock: update returns 204 No Content
        Mock::given(method("PUT"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        // Second mock: get issue to fetch updated details
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/TEST-123"))
            .and(header(
                "Authorization",
                "Basic dGVzdEB0ZXN0LmNvbTp0ZXN0LXRva2Vu",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(mock_jira_issue("TEST-123", "Updated summary")),
            )
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let update = UpdateJiraIssue {
            fields: UpdateJiraIssueFields {
                summary: Some("Updated summary".to_string()),
                description: None,
                priority: None,
                labels: None,
            },
        };

        let issue = client.update_issue("TEST-123", &update).unwrap();
        assert_eq!(issue.fields.summary, "Updated summary");
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
            crate::error::JiraError::Unauthorized
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
        let result = client.get_issue("NONEXISTENT-999");

        assert!(result.is_err());
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
    async fn test_text_to_adf_conversion() {
        // Test that we can convert plain text to ADF format
        let adf = text_to_adf("Hello World");

        assert_eq!(adf["type"], "doc");
        assert_eq!(adf["version"], 1);
        assert!(adf["content"].is_array());
    }

    #[tokio::test]
    async fn test_search_with_pagination() {
        let mock_server = MockServer::start().await;

        // The new Jira API uses GET /search/jql with query parameters
        Mock::given(method("GET"))
            .and(path("/rest/api/3/search/jql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "startAt": 10,
                "maxResults": 5,
                "total": 25,
                "issues": [
                    mock_jira_issue("TEST-11", "Issue 11"),
                    mock_jira_issue("TEST-12", "Issue 12"),
                    mock_jira_issue("TEST-13", "Issue 13"),
                    mock_jira_issue("TEST-14", "Issue 14"),
                    mock_jira_issue("TEST-15", "Issue 15")
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = JiraClient::new(&mock_server.uri(), "test@test.com", "test-token");
        let result = client.search_issues("project = TEST", 5, 10).unwrap();

        assert_eq!(result.start_at, 10);
        assert_eq!(result.max_results, 5);
        assert_eq!(result.total, 25);
        assert_eq!(result.issues.len(), 5);
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
}
