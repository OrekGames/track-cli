//! Unit tests for LinearClient using wiremock.

#[cfg(test)]
mod tests {
    use crate::client::LinearClient;
    use crate::error::LinearError;
    use tracker_core::{IssueTracker, TrackerError};
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn mock_linear_issue(identifier: &str, title: &str) -> serde_json::Value {
        serde_json::json!({
            "id": format!("issue-{}", identifier.to_lowercase()),
            "identifier": identifier,
            "title": title,
            "description": "Test description",
            "priority": 2,
            "priorityLabel": "High",
            "url": format!("https://linear.app/acme/issue/{identifier}"),
            "createdAt": "2024-01-15T10:30:00Z",
            "updatedAt": "2024-01-15T12:00:00Z",
            "team": {
                "id": "team-1",
                "key": "ORE",
                "name": "Orek",
                "description": "Orek team"
            },
            "state": {
                "id": "state-started",
                "name": "In Progress",
                "type": "started",
                "position": 2.0
            },
            "assignee": {
                "id": "user-1",
                "name": "Ada",
                "displayName": "Ada Lovelace",
                "email": "ada@example.com"
            },
            "project": {
                "id": "project-1",
                "name": "Track CLI",
                "slugId": "track-cli",
                "description": null
            },
            "parent": null,
            "labels": {
                "nodes": [
                    {
                        "id": "label-bug",
                        "name": "Bug",
                        "color": "#d73a4a",
                        "description": "Bug reports"
                    }
                ],
                "pageInfo": {
                    "hasNextPage": false,
                    "endCursor": null
                }
            }
        })
    }

    fn mock_team(key: &str) -> serde_json::Value {
        serde_json::json!({
            "id": format!("team-{}", key.to_lowercase()),
            "key": key,
            "name": format!("{key} Team"),
            "description": "Team description"
        })
    }

    fn mock_comment(id: usize) -> serde_json::Value {
        serde_json::json!({
            "id": format!("comment-{id}"),
            "body": format!("Comment {id}"),
            "createdAt": "2024-01-15T10:30:00Z",
            "user": null
        })
    }

    fn request_variables(request: &wiremock::Request) -> serde_json::Value {
        request.body_json::<serde_json::Value>().unwrap()["variables"].clone()
    }

    struct SearchAllIssuePages;

    impl wiremock::Respond for SearchAllIssuePages {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            let variables = request_variables(request);
            let (nodes, has_next_page, end_cursor) = match variables["after"].as_str() {
                None => (
                    (1..=100)
                        .map(|idx| {
                            mock_linear_issue(&format!("ORE-{idx}"), &format!("Issue {idx}"))
                        })
                        .collect::<Vec<_>>(),
                    true,
                    Some("cursor-1"),
                ),
                Some("cursor-1") => (
                    (101..=120)
                        .map(|idx| {
                            mock_linear_issue(&format!("ORE-{idx}"), &format!("Issue {idx}"))
                        })
                        .collect::<Vec<_>>(),
                    true,
                    Some("cursor-2"),
                ),
                other => panic!("unexpected Linear search cursor: {other:?}"),
            };

            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issues": {
                        "nodes": nodes,
                        "pageInfo": {
                            "hasNextPage": has_next_page,
                            "endCursor": end_cursor
                        }
                    }
                }
            }))
        }
    }

    struct NonAdvancingSearchPages;

    impl wiremock::Respond for NonAdvancingSearchPages {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            let variables = request_variables(request);
            let (nodes, end_cursor) = match variables["after"].as_str() {
                None => (vec![mock_linear_issue("ORE-1", "Issue 1")], "cursor-1"),
                Some("cursor-1") => (vec![mock_linear_issue("ORE-2", "Issue 2")], "cursor-1"),
                other => panic!("unexpected Linear search cursor: {other:?}"),
            };

            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issues": {
                        "nodes": nodes,
                        "pageInfo": {
                            "hasNextPage": true,
                            "endCursor": end_cursor
                        }
                    }
                }
            }))
        }
    }

    #[tokio::test]
    async fn test_get_issue_uses_graphql_auth_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("Authorization", "test-token"))
            .and(header("Content-Type", "application/json"))
            .and(body_string_contains("query Issue"))
            .and(body_string_contains("ORE-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": mock_linear_issue("ORE-123", "Found a bug")
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let issue = client.get_issue("ORE-123").unwrap();

        assert_eq!(issue.identifier, "ORE-123");
        assert_eq!(issue.title, "Found a bug");
        assert_eq!(issue.team.key, "ORE");
        assert_eq!(issue.labels.nodes[0].name, "Bug");
    }

    #[tokio::test]
    async fn test_graphql_errors_fail_even_with_http_200() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": null
                },
                "errors": [
                    {
                        "message": "Cannot query field",
                        "extensions": {
                            "code": "GRAPHQL_VALIDATION_FAILED"
                        }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let err = client.get_issue("ORE-1").unwrap_err();

        match err {
            LinearError::Api { status, message } => {
                assert_eq!(status, 200);
                assert!(message.contains("Cannot query field"));
            }
            other => panic!("expected API error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_graphql_rate_limit_maps_to_rate_limited() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "errors": [
                    {
                        "message": "Rate limited",
                        "extensions": {
                            "code": "RATELIMITED"
                        }
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let err = client.get_issue("ORE-1").unwrap_err();

        assert!(matches!(err, LinearError::RateLimited));
    }

    #[tokio::test]
    async fn test_unauthorized_maps_to_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "errors": [{ "message": "Unauthorized" }]
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "bad-token");
        let err = client.get_issue("ORE-1").unwrap_err();

        assert!(matches!(err, LinearError::Unauthorized));
    }

    #[tokio::test]
    async fn test_list_projects_maps_teams() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("query Teams"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "teams": {
                        "nodes": [
                            mock_team("ORE"),
                            mock_team("ENG")
                        ],
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let projects = <LinearClient as IssueTracker>::list_projects(&client).unwrap();

        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].short_name, "ORE");
        assert_eq!(projects[0].id, "team-ore");
    }

    #[tokio::test]
    async fn test_trait_search_preserves_offset() {
        let mock_server = MockServer::start().await;
        let issues: Vec<_> = (1..=50)
            .map(|idx| mock_linear_issue(&format!("ORE-{idx}"), &format!("Issue {idx}")))
            .collect();

        Mock::given(method("POST"))
            .and(body_string_contains("query Issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issues": {
                        "nodes": issues,
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let result =
            <LinearClient as IssueTracker>::search_issues(&client, "#Unresolved", 20, 25).unwrap();

        assert_eq!(result.items.len(), 20);
        assert_eq!(result.items[0].id_readable, "ORE-26");
        assert_eq!(result.items[19].id_readable, "ORE-45");
        assert_eq!(result.total, None);
    }

    #[tokio::test]
    async fn test_search_all_issues_walks_cursor_chain_once_and_respects_max_results() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(SearchAllIssuePages)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let result =
            <LinearClient as IssueTracker>::search_all_issues(&client, "#Unresolved", 120).unwrap();

        assert_eq!(result.len(), 120);
        assert_eq!(result[0].id_readable, "ORE-1");
        assert_eq!(result[119].id_readable, "ORE-120");

        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(
            requests.len(),
            2,
            "expected a native cursor walk, got {requests:#?}"
        );
        let first_request = request_variables(&requests[0]);
        assert_eq!(first_request["after"], serde_json::Value::Null);
        assert_eq!(first_request["first"], 100);

        let second_request = request_variables(&requests[1]);
        assert_eq!(second_request["after"], "cursor-1");
        assert_eq!(second_request["first"], 20);
    }

    #[tokio::test]
    async fn test_search_all_issues_errors_on_non_advancing_cursor() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(NonAdvancingSearchPages)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let result = <LinearClient as IssueTracker>::search_all_issues(&client, "#Unresolved", 10);

        assert!(matches!(result, Err(TrackerError::PaginationStalled(_))));
    }

    #[tokio::test]
    async fn test_project_custom_fields_are_synthetic_team_schema() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("query TeamDetails"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "team": {
                        "id": "team-ore",
                        "key": "ORE",
                        "name": "Orek",
                        "description": null,
                        "states": {
                            "nodes": [
                                { "id": "state-1", "name": "Todo", "type": "unstarted", "position": 1.0 },
                                { "id": "state-2", "name": "Done", "type": "completed", "position": 2.0 }
                            ],
                            "pageInfo": { "hasNextPage": false, "endCursor": null }
                        },
                        "members": {
                            "nodes": [
                                { "id": "user-1", "name": "Ada", "displayName": "Ada", "email": "ada@example.com" }
                            ],
                            "pageInfo": { "hasNextPage": false, "endCursor": null }
                        },
                        "labels": {
                            "nodes": [
                                { "id": "label-bug", "name": "Bug", "color": "#d73a4a", "description": null }
                            ],
                            "pageInfo": { "hasNextPage": false, "endCursor": null }
                        },
                        "projects": {
                            "nodes": [
                                { "id": "project-1", "name": "Track CLI", "slugId": "track-cli", "description": null }
                            ],
                            "pageInfo": { "hasNextPage": false, "endCursor": null }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let fields =
            <LinearClient as IssueTracker>::get_project_custom_fields(&client, "team-ore").unwrap();

        assert!(fields.iter().any(|field| field.name == "Status"));
        assert!(fields.iter().any(|field| field.name == "Assignee"));
        assert!(fields.iter().any(|field| field.name == "Priority"));
        assert!(fields.iter().any(|field| field.name == "Labels"));
        assert!(fields.iter().any(|field| field.name == "Project"));
    }

    #[tokio::test]
    async fn test_get_all_comments_walks_cursor_pages_once() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("query Issue"))
            .and(body_string_contains("ORE-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "issue": mock_linear_issue("ORE-123", "Comment issue") }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("query IssueComments"))
            .and(body_string_contains("cursor-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "comments": {
                            "nodes": [mock_comment(101)],
                            "pageInfo": { "hasNextPage": false, "endCursor": null }
                        }
                    }
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let first_page: Vec<_> = (1..=100).map(mock_comment).collect();
        Mock::given(method("POST"))
            .and(body_string_contains("query IssueComments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "comments": {
                            "nodes": first_page,
                            "pageInfo": { "hasNextPage": true, "endCursor": "cursor-1" }
                        }
                    }
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let comments = <LinearClient as IssueTracker>::get_all_comments(&client, "ORE-123", 101)
            .expect("get_all_comments should succeed");

        assert_eq!(comments.len(), 101);
        assert_eq!(comments[0].id, "comment-1");
        assert_eq!(comments[100].id, "comment-101");
    }

    #[tokio::test]
    async fn test_get_all_comments_errors_when_cursor_does_not_advance() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("query Issue"))
            .and(body_string_contains("ORE-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "issue": mock_linear_issue("ORE-123", "Comment issue") }
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("query IssueComments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "comments": {
                            "nodes": [mock_comment(1)],
                            "pageInfo": { "hasNextPage": true, "endCursor": null }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let result = <LinearClient as IssueTracker>::get_all_comments(&client, "ORE-123", 10);

        assert!(matches!(result, Err(TrackerError::PaginationStalled(_))));
    }

    #[tokio::test]
    async fn test_get_all_comments_errors_when_page_makes_no_progress() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("query Issue"))
            .and(body_string_contains("ORE-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "issue": mock_linear_issue("ORE-123", "Comment issue") }
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("query IssueComments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "comments": {
                            "nodes": [],
                            "pageInfo": { "hasNextPage": true, "endCursor": "cursor-1" }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let result = <LinearClient as IssueTracker>::get_all_comments(&client, "ORE-123", 10);

        assert!(matches!(result, Err(TrackerError::PaginationStalled(_))));
    }

    #[tokio::test]
    async fn test_get_issue_history_paginates_and_sorts() {
        let mock_server = MockServer::start().await;

        // 1. `get_issue` resolves the readable id to the internal id; the
        //    history query then keys on that internal id.
        Mock::given(method("POST"))
            .and(body_string_contains("query Issue"))
            .and(body_string_contains("ORE-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "issue": mock_linear_issue("ORE-123", "History issue") }
            })))
            .mount(&mock_server)
            .await;

        // 2. History page 2 (matched first, since it is more specific): it is
        //    requested with the cursor from page 1 and reports the newest node.
        Mock::given(method("POST"))
            .and(body_string_contains("query IssueHistory"))
            .and(body_string_contains("cursor-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "history": {
                            "nodes": [
                                {
                                    "createdAt": "2024-01-15T14:30:00Z",
                                    "actor": { "id": "u-bob", "name": "bob", "displayName": "Bob", "email": "bob@example.com" },
                                    "fromState": { "name": "In Progress" },
                                    "toState": { "name": "Done" }
                                }
                            ],
                            "pageInfo": { "hasNextPage": false, "endCursor": null }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        // 3. History page 1 (no cursor): an older node, with `hasNextPage` true
        //    so the loop must fetch page 2.
        Mock::given(method("POST"))
            .and(body_string_contains("query IssueHistory"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "history": {
                            "nodes": [
                                {
                                    "createdAt": "2024-01-10T09:00:00Z",
                                    "actor": { "id": "u-alice", "name": "alice", "displayName": "Alice", "email": "alice@example.com" },
                                    "fromState": { "name": "Todo" },
                                    "toState": { "name": "In Progress" }
                                }
                            ],
                            "pageInfo": { "hasNextPage": true, "endCursor": "cursor-1" }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let events = <LinearClient as IssueTracker>::get_issue_history(&client, "ORE-123").unwrap();

        // One event per page, both state transitions canonicalized to "status".
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.field == "status"));

        // Newest-first: the page-2 (Done) node sorts to the front.
        assert_eq!(events[0].to.as_deref(), Some("Done"));
        assert_eq!(events[0].from.as_deref(), Some("In Progress"));
        assert_eq!(
            events[0].author.as_ref().and_then(|a| a.name.as_deref()),
            Some("Bob")
        );
        // Oldest sorts last.
        assert_eq!(events[1].to.as_deref(), Some("In Progress"));
        assert_eq!(events[1].from.as_deref(), Some("Todo"));
    }

    #[tokio::test]
    async fn test_get_issue_history_skips_empty_page_with_next_cursor() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("query Issue"))
            .and(body_string_contains("ORE-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "issue": mock_linear_issue("ORE-123", "History issue") }
            })))
            .mount(&mock_server)
            .await;

        // Page 3: real data, last page.
        Mock::given(method("POST"))
            .and(body_string_contains("query IssueHistory"))
            .and(body_string_contains("cursor-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "history": {
                            "nodes": [
                                {
                                    "createdAt": "2024-01-15T14:30:00Z",
                                    "fromState": { "name": "In Progress" },
                                    "toState": { "name": "Done" }
                                }
                            ],
                            "pageInfo": { "hasNextPage": false, "endCursor": null }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        // Page 2: empty but hasNextPage — must not truncate.
        Mock::given(method("POST"))
            .and(body_string_contains("query IssueHistory"))
            .and(body_string_contains("cursor-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "history": {
                            "nodes": [],
                            "pageInfo": { "hasNextPage": true, "endCursor": "cursor-2" }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        // Page 1: oldest node with a next cursor.
        Mock::given(method("POST"))
            .and(body_string_contains("query IssueHistory"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "history": {
                            "nodes": [
                                {
                                    "createdAt": "2024-01-10T09:00:00Z",
                                    "fromState": { "name": "Todo" },
                                    "toState": { "name": "In Progress" }
                                }
                            ],
                            "pageInfo": { "hasNextPage": true, "endCursor": "cursor-1" }
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let events = <LinearClient as IssueTracker>::get_issue_history(&client, "ORE-123").unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].to.as_deref(), Some("Done"));
        assert_eq!(events[1].to.as_deref(), Some("In Progress"));
    }
}
