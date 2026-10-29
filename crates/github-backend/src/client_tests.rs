//! Unit tests for GitHubClient using wiremock

#[cfg(test)]
mod tests {
    use crate::client::GitHubClient;
    use tracker_core::{
        AttachmentUpload, CreateIssue, Issue, IssueTracker, TrackerError, UpdateIssue,
    };
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Serve `response` for issue 1 and fetch it through the `IssueTracker` trait.
    async fn trait_get_issue_with(response: ResponseTemplate) -> tracker_core::Result<Issue> {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1"))
            .respond_with(response)
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        <GitHubClient as IssueTracker>::get_issue(&client, "1")
    }

    /// Unwrap a `TrackerError::Api` into its status and message.
    fn api_error(err: TrackerError) -> (u16, String) {
        match err {
            TrackerError::Api { status, message } => (status, message),
            other => panic!("Expected Api error, got: {:?}", other),
        }
    }

    /// A body one byte over ureq's default 10 MB read limit.
    fn oversized_body() -> String {
        "x".repeat(10 * 1024 * 1024 + 1)
    }

    /// A localhost URL nothing listens on.
    fn closed_localhost_url() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{addr}")
    }

    /// Helper to create a mock GitHub issue response
    fn mock_github_issue(number: u64, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": 1000 + number,
            "number": number,
            "title": title,
            "body": "Test description",
            "state": "open",
            "labels": [
                {"id": 1, "name": "bug", "color": "fc2929", "description": "Something isn't working"},
                {"id": 2, "name": "enhancement", "color": "84b6eb", "description": null}
            ],
            "assignee": {"login": "testuser", "id": 1},
            "assignees": [{"login": "testuser", "id": 1}],
            "milestone": {"id": 1, "number": 1, "title": "v1.0"},
            "created_at": "2024-01-15T10:30:00Z",
            "updated_at": "2024-01-15T12:00:00Z",
            "closed_at": null,
            "user": {"login": "reporter", "id": 2},
            "pull_request": null
        })
    }

    /// Helper to create a mock GitHub PR disguised as issue
    fn mock_github_pr(number: u64, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": 2000 + number,
            "number": number,
            "title": title,
            "body": "PR description",
            "state": "open",
            "labels": [],
            "assignee": null,
            "assignees": [],
            "milestone": null,
            "created_at": "2024-01-15T10:30:00Z",
            "updated_at": "2024-01-15T12:00:00Z",
            "closed_at": null,
            "user": {"login": "developer", "id": 3},
            "pull_request": {
                "url": "https://api.github.com/repos/owner/repo/pulls/99"
            }
        })
    }

    /// Helper to create a mock GitHub repo response
    fn mock_github_repo(name: &str, full_name: &str) -> serde_json::Value {
        serde_json::json!({
            "id": 12345,
            "name": name,
            "full_name": full_name,
            "description": "Test repository",
            "owner": {"login": "owner", "id": 1}
        })
    }

    #[tokio::test]
    async fn test_get_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/42"))
            .and(header("Authorization", "Bearer test-token"))
            .and(header("Accept", "application/vnd.github+json"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_github_issue(42, "Found a bug")),
            )
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let issue = client.get_issue(42).unwrap();

        assert_eq!(issue.number, 42);
        assert_eq!(issue.title, "Found a bug");
        assert_eq!(issue.state, "open");
        assert_eq!(issue.labels.len(), 2);
        assert_eq!(issue.labels[0].name, "bug");
        assert!(issue.assignee.is_some());
        assert_eq!(issue.assignee.as_ref().unwrap().login, "testuser");
        assert!(issue.milestone.is_some());
        assert_eq!(issue.milestone.as_ref().unwrap().title, "v1.0");
        assert!(!issue.is_pull_request());
    }

    #[tokio::test]
    async fn test_search_issues() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/search/issues"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 2,
                "incomplete_results": false,
                "items": [
                    mock_github_issue(10, "Search result 1"),
                    mock_github_issue(11, "Search result 2")
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let result = client.search_issues("is:open label:bug", 30, 1).unwrap();

        assert_eq!(result.total_count, 2);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].title, "Search result 1");
        assert_eq!(result.items[1].title, "Search result 2");
    }

    #[tokio::test]
    async fn test_search_issues_filters_prs() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/search/issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 3,
                "incomplete_results": false,
                "items": [
                    mock_github_issue(10, "Issue"),
                    mock_github_pr(20, "Pull request"),
                    mock_github_issue(30, "Another issue")
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let result = client.search_issues("is:open", 30, 1).unwrap();

        // Search result itself contains all items including PRs
        assert_eq!(result.total_count, 3);
        assert_eq!(result.items.len(), 3);
        // But we can filter at the trait_impl level; here we verify the PR flag
        assert!(!result.items[0].is_pull_request());
        assert!(result.items[1].is_pull_request());
        assert!(!result.items[2].is_pull_request());
    }

    #[tokio::test]
    async fn test_trait_search_issues_preserves_offset() {
        let mock_server = MockServer::start().await;
        let items: Vec<_> = (1..=50)
            .map(|number| mock_github_issue(number, &format!("Search result {number}")))
            .collect();

        Mock::given(method("GET"))
            .and(path("/search/issues"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 50,
                "incomplete_results": false,
                "items": items
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let result =
            <GitHubClient as IssueTracker>::search_issues(&client, "is:open", 20, 25).unwrap();

        assert_eq!(result.items.len(), 20);
        assert_eq!(result.items[0].id_readable, "owner/repo#26");
        assert_eq!(result.items[19].id_readable, "owner/repo#45");
    }

    #[tokio::test]
    async fn test_count_issues() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/search/issues"))
            .and(query_param("per_page", "1"))
            .and(query_param("page", "1"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 750,
                "incomplete_results": false,
                "items": []
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let count = client
            .count_issues("project: owner/repo #Unresolved")
            .unwrap();

        assert_eq!(count, 750);
    }

    #[tokio::test]
    async fn test_create_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/issues"))
            .and(header("Authorization", "Bearer test-token"))
            .and(header("Content-Type", "application/json"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(mock_github_issue(50, "New issue")),
            )
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");

        let create = crate::models::CreateGitHubIssue {
            title: "New issue".to_string(),
            body: Some("Issue description".to_string()),
            labels: Some(vec!["bug".to_string()]),
            assignees: None,
            milestone: None,
        };

        let issue = client.create_issue(&create).unwrap();
        assert_eq!(issue.title, "New issue");
    }

    #[tokio::test]
    async fn test_update_issue() {
        let mock_server = MockServer::start().await;

        let mut updated_issue = mock_github_issue(42, "Updated title");
        updated_issue["state"] = serde_json::json!("closed");
        updated_issue["closed_at"] = serde_json::json!("2024-01-16T10:00:00Z");

        Mock::given(method("PATCH"))
            .and(path("/repos/owner/repo/issues/42"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(updated_issue))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");

        let update = crate::models::UpdateGitHubIssue {
            title: Some("Updated title".to_string()),
            body: None,
            state: Some("closed".to_string()),
            labels: None,
            assignees: None,
            milestone: None,
        };

        let issue = client.update_issue(42, &update).unwrap();
        assert_eq!(issue.title, "Updated title");
        assert_eq!(issue.state, "closed");
    }

    #[tokio::test]
    async fn test_list_repos() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/user/repos"))
            .and(query_param("per_page", "100"))
            .and(query_param("sort", "updated"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                mock_github_repo("repo-one", "owner/repo-one"),
                mock_github_repo("repo-two", "owner/repo-two")
            ])))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let repos = client.list_repos().unwrap();

        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name, "repo-one");
        assert_eq!(repos[0].full_name, "owner/repo-one");
        assert_eq!(repos[1].name, "repo-two");
    }

    #[tokio::test]
    async fn test_get_repo() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/my-repo"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(mock_github_repo("my-repo", "owner/my-repo")),
            )
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let repo = client.get_repo("owner", "my-repo").unwrap();

        assert_eq!(repo.name, "my-repo");
        assert_eq!(repo.full_name, "owner/my-repo");
        assert_eq!(repo.description, Some("Test repository".to_string()));
    }

    #[tokio::test]
    async fn test_list_labels() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/labels"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": 1, "name": "bug", "color": "fc2929", "description": "Something isn't working"},
                {"id": 2, "name": "enhancement", "color": "84b6eb", "description": "New feature"},
                {"id": 3, "name": "documentation", "color": "0075ca", "description": null}
            ])))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let labels = client.list_labels().unwrap();

        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0].name, "bug");
        assert_eq!(labels[0].color, "fc2929");
        assert_eq!(labels[1].name, "enhancement");
        assert_eq!(labels[2].name, "documentation");
    }

    #[tokio::test]
    async fn test_list_labels_pages_past_100() {
        let mock_server = MockServer::start().await;
        let label =
            |i: u64| serde_json::json!({"id": i, "name": format!("label-{i}"), "color": "ededed"});

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/labels"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([label(101)])))
            .with_priority(1)
            .mount(&mock_server)
            .await;
        // Any other request (the first page) gets a full page of 100 labels.
        let first_page: Vec<_> = (1..=100).map(label).collect();
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/labels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(first_page))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let labels = client.list_labels().unwrap();

        assert_eq!(labels.len(), 101);
        assert_eq!(labels[100].name, "label-101");
    }

    #[tokio::test]
    async fn test_create_label() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/test-owner/test-repo/labels"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 100,
                "name": "type: bug",
                "color": "d73a4a",
                "description": "Something broken"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(
            &mock_server.uri(),
            "test-owner",
            "test-repo",
            "test-token",
        );
        let label = crate::models::CreateGitHubLabel {
            name: "type: bug".to_string(),
            color: "d73a4a".to_string(),
            description: Some("Something broken".to_string()),
        };
        let result = client.create_label(&label).unwrap();
        assert_eq!(result.name, "type: bug");
        assert_eq!(result.color, "d73a4a");
    }

    #[tokio::test]
    async fn test_delete_label() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/repos/test-owner/test-repo/labels/type%3A%20bug"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(
            &mock_server.uri(),
            "test-owner",
            "test-repo",
            "test-token",
        );
        let result = client.delete_label("type: bug");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_label() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/repos/test-owner/test-repo/labels/old-name"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 100,
                "name": "new-name",
                "color": "0075ca",
                "description": "Updated description"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(
            &mock_server.uri(),
            "test-owner",
            "test-repo",
            "test-token",
        );
        let update = crate::models::UpdateGitHubLabel {
            new_name: Some("new-name".to_string()),
            color: Some("0075ca".to_string()),
            description: Some("Updated description".to_string()),
        };
        let result = client.update_label("old-name", &update).unwrap();
        assert_eq!(result.name, "new-name");
        assert_eq!(result.color, "0075ca");
    }

    #[tokio::test]
    async fn test_add_comment() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/issues/42/comments"))
            .and(header("Authorization", "Bearer test-token"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 100,
                "body": "This is my comment",
                "user": {"login": "testuser", "id": 1},
                "created_at": "2024-01-15T14:00:00Z",
                "updated_at": "2024-01-15T14:00:00Z"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let comment = client.add_comment(42, "This is my comment").unwrap();

        assert_eq!(comment.id, 100);
        assert_eq!(comment.body, "This is my comment");
        assert!(comment.user.is_some());
        assert_eq!(comment.user.unwrap().login, "testuser");
    }

    #[tokio::test]
    async fn test_get_comments() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/42/comments"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "1"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 100,
                    "body": "First comment",
                    "user": {"login": "user1", "id": 1},
                    "created_at": "2024-01-15T10:00:00Z",
                    "updated_at": "2024-01-15T10:00:00Z"
                },
                {
                    "id": 101,
                    "body": "Second comment",
                    "user": {"login": "user2", "id": 2},
                    "created_at": "2024-01-15T11:00:00Z",
                    "updated_at": "2024-01-15T11:00:00Z"
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let comments = client.get_comments(42).unwrap();

        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].id, 100);
        assert_eq!(comments[0].body, "First comment");
        assert_eq!(comments[1].id, 101);
        assert_eq!(comments[1].body, "Second comment");
    }

    #[tokio::test]
    async fn test_get_issue_history_paginates_and_derives_status() {
        let mock_server = MockServer::start().await;

        // Page 1: a full page of 100 events (oldest-first). A full page forces
        // the loop to request a second page. These are all `labeled` events so
        // they don't disturb the running status; timestamps ascend.
        let page1: Vec<serde_json::Value> = (0..100)
            .map(|i| {
                // Keep hours/minutes in valid RFC3339 ranges (<24 / <60).
                let created_at = format!("2024-01-01T{:02}:{:02}:00Z", i / 60, i % 60);
                serde_json::json!({
                    "event": "labeled",
                    "created_at": created_at,
                    "actor": { "login": "alice", "id": 1 },
                    "label": { "name": format!("label-{i}"), "color": "ffffff" }
                })
            })
            .collect();

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/42/timeline"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "1"))
            .and(header("Authorization", "Bearer test-token"))
            .and(header("X-GitHub-Api-Version", "2022-11-28"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(page1)))
            .mount(&mock_server)
            .await;

        // Page 2: a short page (< per_page) signaling end-of-data. A
        // closed->reopened->closed sequence exercises the running-status
        // derivation, plus an unknown event that must be ignored.
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/42/timeline"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "event": "closed",
                    "created_at": "2024-02-01T10:00:00Z",
                    "actor": { "login": "bob", "id": 2 }
                },
                {
                    "event": "reopened",
                    "created_at": "2024-02-02T10:00:00Z",
                    "actor": { "login": "carol", "id": 3 }
                },
                {
                    "event": "commented",
                    "created_at": "2024-02-03T10:00:00Z",
                    "actor": { "login": "dave", "id": 4 }
                },
                {
                    "event": "closed",
                    "created_at": "2024-02-04T10:00:00Z",
                    "actor": { "login": "erin", "id": 5 }
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let events = client.get_issue_history("42").unwrap();

        // 100 labels (page 1) + 3 status events (page 2, comment dropped).
        assert_eq!(events.len(), 103);

        // Newest-first: the page-2 status events sort to the front in reverse
        // chronological order. Verify the from-walk: open->closed,
        // closed->open, open->closed; reversed for display.
        assert_eq!(events[0].field, "status");
        assert_eq!(events[0].from.as_deref(), Some("open"));
        assert_eq!(events[0].to.as_deref(), Some("closed"));
        assert_eq!(events[0].author.as_ref().unwrap().login, "erin");

        assert_eq!(events[1].field, "status");
        assert_eq!(events[1].from.as_deref(), Some("closed"));
        assert_eq!(events[1].to.as_deref(), Some("open"));
        assert_eq!(events[1].author.as_ref().unwrap().login, "carol");

        assert_eq!(events[2].field, "status");
        assert_eq!(events[2].from.as_deref(), Some("open"));
        assert_eq!(events[2].to.as_deref(), Some("closed"));
        assert_eq!(events[2].author.as_ref().unwrap().login, "bob");

        // The remaining 100 are the labels.
        assert!(events[3..].iter().all(|e| e.field == "labels"));
    }

    #[tokio::test]
    async fn test_rate_limit_detection() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("x-ratelimit-limit", "60")
                    .set_body_json(serde_json::json!({
                        "message": "API rate limit exceeded",
                        "documentation_url": "https://docs.github.com/rest"
                    })),
            )
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let result = client.get_issue(1);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::GitHubError::RateLimited
        ));
    }

    #[tokio::test]
    async fn test_unauthorized_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Bad credentials",
                "documentation_url": "https://docs.github.com/rest"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "bad-token");
        let result = client.get_issue(1);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::GitHubError::Unauthorized
        ));
    }

    #[tokio::test]
    async fn test_forbidden_without_rate_limit_is_api_error() {
        let mock_server = MockServer::start().await;

        // 403 without x-ratelimit-remaining: 0 should be a regular API error, not RateLimited
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "59")
                    .set_body_json(serde_json::json!({
                        "message": "Resource not accessible by integration"
                    })),
            )
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let result = client.get_issue(1);

        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::GitHubError::Api { status, message } => {
                assert_eq!(status, 403);
                assert!(message.contains("Resource not accessible"));
            }
            other => panic!("Expected Api error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_invalid_success_body_is_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let result = client.get_issue(1);

        // A body that is not the expected JSON is a decode failure, not a
        // transport (`Http`) failure.
        match result.unwrap_err() {
            crate::error::GitHubError::Parse(_) => {}
            other => panic!("Expected Parse error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/99999"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "Not Found",
                "documentation_url": "https://docs.github.com/rest"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let result = client.get_issue(99999);

        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::GitHubError::Api { status, .. } => {
                assert_eq!(status, 404);
            }
            other => panic!("Expected Api error, got: {:?}", other),
        }
    }

    // ==================== Error Mapping Through the Traits ====================

    #[tokio::test]
    async fn test_trait_rate_limit_is_api_429() {
        let err = trait_get_issue_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .set_body_json(serde_json::json!({
                    "message": "API rate limit exceeded"
                })),
        )
        .await
        .unwrap_err();

        // The real status (403) and GitHub's message are both replaced.
        assert_eq!(
            api_error(err),
            (429, "GitHub API rate limit exceeded".to_string())
        );
    }

    #[tokio::test]
    async fn test_real_429_is_api_with_body_message() {
        let err =
            trait_get_issue_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "message": "secondary rate limit"
            })))
            .await
            .unwrap_err();

        // Only a 403 with x-ratelimit-remaining: 0 counts as rate limiting.
        assert_eq!(api_error(err), (429, "secondary rate limit".to_string()));
    }

    #[tokio::test]
    async fn test_error_json_without_message_uses_raw_body() {
        let body = r#"{"errors":[]}"#;
        let err =
            trait_get_issue_with(ResponseTemplate::new(500).set_body_raw(body, "application/json"))
                .await
                .unwrap_err();

        assert_eq!(api_error(err), (500, body.to_string()));
    }

    #[tokio::test]
    async fn test_error_json_array_or_non_string_message_uses_raw_body() {
        for body in [r#"["boom"]"#, r#"{"message":42}"#, r#""boom""#] {
            let err = trait_get_issue_with(
                ResponseTemplate::new(500).set_body_raw(body, "application/json"),
            )
            .await
            .unwrap_err();

            assert_eq!(api_error(err), (500, body.to_string()), "body {body}");
        }
    }

    #[tokio::test]
    async fn test_non_json_error_body_is_used_verbatim() {
        let body = "  upstream connect error\n";
        let err = trait_get_issue_with(ResponseTemplate::new(503).set_body_string(body))
            .await
            .unwrap_err();

        // Not trimmed.
        assert_eq!(api_error(err), (503, body.to_string()));
    }

    #[tokio::test]
    async fn test_empty_error_body_is_http_status() {
        let err = trait_get_issue_with(ResponseTemplate::new(502))
            .await
            .unwrap_err();

        assert_eq!(api_error(err), (502, "HTTP 502".to_string()));
    }

    #[tokio::test]
    async fn test_unreadable_error_body_falls_back_to_http_status() {
        let err =
            trait_get_issue_with(ResponseTemplate::new(500).set_body_string(oversized_body()))
                .await
                .unwrap_err();

        // The read failure is swallowed as an empty body, not reported as Http.
        let (status, message) = api_error(err);
        assert_eq!(status, 500);
        assert!(
            message == "HTTP 500",
            "expected \"HTTP 500\", got {} bytes",
            message.len()
        );
    }

    #[tokio::test]
    async fn test_validation_errors_array_is_dropped() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/issues"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "message": "Validation Failed",
                "errors": [{"field": "title", "code": "missing"}],
                "documentation_url": "u"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let create = CreateIssue {
            project_id: "owner/repo".to_string(),
            summary: String::new(),
            description: None,
            custom_fields: Vec::new(),
            tags: Vec::new(),
            parent: None,
        };
        let err = <GitHubClient as IssueTracker>::create_issue(&client, &create).unwrap_err();

        // Only `message` survives; the field-level detail is lost.
        assert_eq!(api_error(err), (422, "Validation Failed".to_string()));
    }

    #[tokio::test]
    async fn test_trait_unauthorized_is_unauthorized() {
        let err =
            trait_get_issue_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Bad credentials"
            })))
            .await
            .unwrap_err();

        assert!(matches!(err, TrackerError::Unauthorized), "{err:?}");
    }

    #[tokio::test]
    async fn test_trait_get_issue_404_is_api_404() {
        let err =
            trait_get_issue_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "Not Found",
                "documentation_url": "https://docs.github.com/rest"
            })))
            .await
            .unwrap_err();

        // Not IssueNotFound or NotFound: only get_project remaps a 404.
        assert_eq!(api_error(err), (404, "Not Found".to_string()));
    }

    #[tokio::test]
    async fn test_trait_get_project_404_is_project_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "Not Found"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let err = <GitHubClient as IssueTracker>::get_project(&client, "owner/repo").unwrap_err();

        assert!(
            matches!(err, TrackerError::ProjectNotFound(ref id) if id == "owner/repo"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn test_trait_get_project_other_errors_pass_through() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "message": "Resource not accessible by integration"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let err = <GitHubClient as IssueTracker>::get_project(&client, "owner/repo").unwrap_err();

        assert_eq!(
            api_error(err),
            (403, "Resource not accessible by integration".to_string())
        );
    }

    #[tokio::test]
    async fn test_trait_get_issue_pull_request_is_issue_not_found() {
        let err = trait_get_issue_with(
            ResponseTemplate::new(200).set_body_json(mock_github_pr(1, "A pull request")),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, TrackerError::IssueNotFound(_)), "{err:?}");
    }

    #[tokio::test]
    async fn test_trait_invalid_success_body_is_parse_error() {
        let err = trait_get_issue_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .await
            .unwrap_err();

        assert!(matches!(err, TrackerError::Parse(_)), "{err:?}");
        assert!(err.to_string().starts_with("Parse error: "), "{err}");
    }

    #[tokio::test]
    async fn test_invalid_success_body_display_is_json_parse_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let err = client.get_issue(1).unwrap_err();

        // `track init` prints this Display in its fallback branch.
        assert!(err.to_string().starts_with("JSON parse error: "), "{err}");
    }

    #[tokio::test]
    async fn test_oversized_success_body_is_http_not_parse() {
        let err =
            trait_get_issue_with(ResponseTemplate::new(200).set_body_string(oversized_body()))
                .await
                .unwrap_err();

        // Failing to read the body is a transport failure, not bad JSON.
        assert!(matches!(err, TrackerError::Http(_)), "{err:?}");
    }

    #[test]
    fn test_transport_failure_is_http() {
        let client =
            GitHubClient::with_base_url(&closed_localhost_url(), "owner", "repo", "test-token");
        let err = <GitHubClient as IssueTracker>::get_issue(&client, "1").unwrap_err();

        assert!(matches!(err, TrackerError::Http(_)), "{err:?}");
    }

    #[tokio::test]
    async fn test_timeline_event_with_wrong_type_is_parse_error() {
        let mock_server = MockServer::start().await;

        // The page decodes as a JSON array; decoding the event inside it fails.
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/42/timeline"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "event": "closed",
                    "created_at": "2024-02-01T10:00:00Z",
                    "actor": "bob"
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let err = client.get_issue_history("42").unwrap_err();

        assert!(matches!(err, TrackerError::Parse(_)), "{err:?}");
    }

    #[test]
    fn test_pagination_stalled_maps_one_to_one() {
        // get_all_pages only builds this after 10_000 full pages.
        let err = TrackerError::from(crate::error::GitHubError::PaginationStalled(
            "'u' did not terminate after 10000 pages".to_string(),
        ));

        assert!(
            matches!(
                err,
                TrackerError::PaginationStalled(ref message)
                    if message == "'u' did not terminate after 10000 pages"
            ),
            "{err:?}"
        );
    }

    #[test]
    fn test_issue_attachments_are_invalid_input() {
        let client = GitHubClient::new("owner", "repo", "test-token");
        let upload = AttachmentUpload {
            files: Vec::new(),
            comment: None,
            silent: false,
            minor_edit: false,
        };

        let results = [
            client.list_issue_attachments("1").map(|_| ()),
            client.add_issue_attachment("1", &upload).map(|_| ()),
            client
                .add_issue_comment_attachment("1", "text", &upload)
                .map(|_| ()),
        ];
        for result in results {
            match result {
                Err(TrackerError::InvalidInput(message)) => {
                    assert!(
                        message.contains("does not expose a public REST API"),
                        "{message}"
                    );
                }
                other => panic!("Expected InvalidInput error, got: {:?}", other),
            }
        }
    }

    // ==================== Request Plumbing ====================

    #[tokio::test]
    async fn test_write_requests_carry_github_headers() {
        let mock_server = MockServer::start().await;

        // An unmatched request gets wiremock's 404 and fails the call.
        for (verb, route, response) in [
            (
                "POST",
                "/repos/owner/repo/issues",
                ResponseTemplate::new(201).set_body_json(mock_github_issue(50, "New issue")),
            ),
            (
                "PATCH",
                "/repos/owner/repo/issues/42",
                ResponseTemplate::new(200).set_body_json(mock_github_issue(42, "Updated title")),
            ),
            (
                "DELETE",
                "/repos/owner/repo/labels/bug",
                ResponseTemplate::new(204),
            ),
        ] {
            Mock::given(method(verb))
                .and(path(route))
                .and(header("Authorization", "Bearer test-token"))
                .and(header("Accept", "application/vnd.github+json"))
                .and(header("X-GitHub-Api-Version", "2022-11-28"))
                .respond_with(response)
                .expect(1)
                .mount(&mock_server)
                .await;
        }

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let create = crate::models::CreateGitHubIssue {
            title: "New issue".to_string(),
            body: None,
            labels: None,
            assignees: None,
            milestone: None,
        };
        let update = crate::models::UpdateGitHubIssue {
            title: Some("Updated title".to_string()),
            body: None,
            state: None,
            labels: None,
            assignees: None,
            milestone: None,
        };

        client.create_issue(&create).unwrap();
        client.update_issue(42, &update).unwrap();
        client.delete_label("bug").unwrap();
    }

    #[tokio::test]
    async fn test_discarded_success_bodies_are_not_decoded() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/issues/10/sub_issues"))
            .respond_with(ResponseTemplate::new(201).set_body_string("not-json"))
            .mount(&mock_server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/repos/owner/repo/labels/bug"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");

        client.add_sub_issue(10, 1042).unwrap();
        client.delete_label("bug").unwrap();
    }

    #[tokio::test]
    async fn test_trait_update_issue_returns_patch_body_without_refetch() {
        let mock_server = MockServer::start().await;

        // GitHub's PATCH response is the updated issue, so no GET follows it.
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/42"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_github_issue(42, "Old title")),
            )
            .expect(0)
            .mount(&mock_server)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/repos/owner/repo/issues/42"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_github_issue(42, "New title")),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let update = UpdateIssue {
            summary: Some("New title".to_string()),
            ..Default::default()
        };
        let issue = <GitHubClient as IssueTracker>::update_issue(&client, "42", &update).unwrap();

        assert_eq!(issue.summary, "New title");
    }

    #[tokio::test]
    async fn test_base_url_trailing_slash_is_trimmed() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/42"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(mock_github_issue(42, "Found a bug")),
            )
            .mount(&mock_server)
            .await;

        let base_url = format!("{}/", mock_server.uri());
        let client = GitHubClient::with_base_url(&base_url, "owner", "repo", "test-token");
        let issue = client.get_issue(42).unwrap();

        assert_eq!(issue.number, 42);
    }

    // ==================== Sub-Issue Operations ====================

    #[tokio::test]
    async fn test_add_sub_issue() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/issues/10/sub_issues"))
            .and(header("Authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "sub_issue_id": 1042,
                "replace_parent": true
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 1042,
                "number": 42,
                "title": "Child issue",
                "state": "open"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let result = client.add_sub_issue(10, 1042);

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_add_sub_issue_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/issues/999/sub_issues"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "Not Found"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::with_base_url(&mock_server.uri(), "owner", "repo", "test-token");
        let result = client.add_sub_issue(999, 1042);

        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::GitHubError::Api { status, .. } => {
                assert_eq!(status, 404);
            }
            other => panic!("Expected Api error, got: {:?}", other),
        }
    }

    #[test]
    fn web_url_points_at_the_web_host_not_the_api() {
        let cases = [
            ("https://api.github.com", "https://github.com/o/r/issues"),
            (
                "https://github.example.com/api/v3",
                "https://github.example.com/o/r/issues",
            ),
            (
                "https://api.octocorp.ghe.com",
                "https://octocorp.ghe.com/o/r/issues",
            ),
        ];
        for (api_url, issues_url) in cases {
            let client = GitHubClient::with_base_url(api_url, "o", "r", "token");
            assert_eq!(
                client.web_url(None).unwrap().as_deref(),
                Some(issues_url),
                "dashboard for API URL {api_url}"
            );
            assert_eq!(
                client.web_url(Some("42")).unwrap(),
                Some(format!("{issues_url}/42")),
                "issue for API URL {api_url}"
            );
        }
    }

    #[test]
    fn web_url_accepts_every_issue_id_form() {
        let client = GitHubClient::with_base_url("https://api.github.com", "o", "r", "token");

        assert_eq!(
            client.web_url(Some("#7")).unwrap().as_deref(),
            Some("https://github.com/o/r/issues/7")
        );
        assert_eq!(
            client.web_url(Some("other/lib#7")).unwrap().as_deref(),
            Some("https://github.com/other/lib/issues/7")
        );
        assert!(matches!(
            client.web_url(Some("PROJ-7")),
            Err(tracker_core::TrackerError::InvalidInput(_))
        ));
    }
}
