//! Unit tests for LinearClient using wiremock.

#[cfg(test)]
mod tests {
    use crate::client::LinearClient;
    use tracker_core::{CreateIssue, CustomFieldUpdate, IssueTracker, TrackerError};
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

    /// A `query TeamConnection` response page; an `end_cursor` means more
    /// pages follow.
    fn team_connection_page(
        nodes: Vec<serde_json::Value>,
        end_cursor: Option<&str>,
    ) -> serde_json::Value {
        serde_json::json!({
            "data": {
                "team": {
                    "connection": {
                        "nodes": nodes,
                        "pageInfo": {
                            "hasNextPage": end_cursor.is_some(),
                            "endCursor": end_cursor
                        }
                    }
                }
            }
        })
    }

    fn mock_user(name: &str) -> serde_json::Value {
        serde_json::json!({
            "id": format!("user-{name}"),
            "name": name,
            "displayName": name,
            "email": format!("{name}@example.com")
        })
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

        // Only the skip + limit window is requested, not a full page.
        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(request_variables(&requests[0])["first"], 45);
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

    #[tokio::test]
    async fn test_find_label_searches_every_page_of_team_labels() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("query TeamConnection"))
            .and(body_string_contains("cursor-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(team_connection_page(
                vec![serde_json::json!({ "id": "label-ui", "name": "UI", "color": null, "description": null })],
                None,
            )))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("query TeamConnection"))
            .respond_with(ResponseTemplate::new(200).set_body_json(team_connection_page(
                vec![serde_json::json!({ "id": "label-bug", "name": "Bug", "color": null, "description": null })],
                Some("cursor-1"),
            )))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let label = client.find_label("team-ore", "ui").unwrap();

        assert_eq!(label.id, "label-ui");
    }

    #[tokio::test]
    async fn test_project_custom_fields_include_members_beyond_first_page() {
        let mock_server = MockServer::start().await;
        let empty_page = serde_json::json!({ "hasNextPage": false, "endCursor": null });

        Mock::given(method("POST"))
            .and(body_string_contains("query TeamDetails"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "team": {
                        "id": "team-ore",
                        "key": "ORE",
                        "name": "Orek",
                        "description": null,
                        "states": { "nodes": [], "pageInfo": empty_page },
                        "members": {
                            "nodes": [mock_user("ada")],
                            "pageInfo": { "hasNextPage": true, "endCursor": "members-1" }
                        },
                        "labels": { "nodes": [], "pageInfo": empty_page },
                        "projects": { "nodes": [], "pageInfo": empty_page }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("query TeamConnection"))
            .and(body_string_contains("members-1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(team_connection_page(vec![mock_user("bob")], None)),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let fields =
            <LinearClient as IssueTracker>::get_project_custom_fields(&client, "team-ore").unwrap();

        let assignee = fields
            .iter()
            .find(|field| field.name == "Assignee")
            .unwrap();
        assert_eq!(assignee.values, ["ada@example.com", "bob@example.com"]);
    }

    #[tokio::test]
    async fn test_list_teams_errors_when_cursor_does_not_advance() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("query Teams"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "teams": {
                        "nodes": [mock_team("ORE")],
                        "pageInfo": { "hasNextPage": true, "endCursor": null }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let err = client.list_teams().unwrap_err();

        assert!(matches!(err, TrackerError::PaginationStalled(_)));
    }

    #[tokio::test]
    async fn test_get_issue_links_includes_relations_pointing_at_the_issue() {
        let mock_server = MockServer::start().await;
        let empty_page = serde_json::json!({ "hasNextPage": false, "endCursor": null });

        Mock::given(method("POST"))
            .and(body_string_contains("query Issue("))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "issue": mock_linear_issue("ORE-2", "Blocked issue") }
            })))
            .mount(&mock_server)
            .await;

        // ORE-1 blocks ORE-2, so Linear lists the relation under ORE-2's
        // `inverseRelations`, not its `relations`.
        Mock::given(method("POST"))
            .and(body_string_contains("query IssueRelations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issue": {
                        "parent": null,
                        "children": { "nodes": [], "pageInfo": empty_page },
                        "relations": { "nodes": [], "pageInfo": empty_page },
                        "inverseRelations": {
                            "nodes": [{
                                "id": "relation-1",
                                "type": "blocks",
                                "issue": { "id": "issue-ore-1", "identifier": "ORE-1", "title": "Blocker" },
                                "relatedIssue": { "id": "issue-ore-2", "identifier": "ORE-2", "title": "Blocked issue" }
                            }],
                            "pageInfo": empty_page
                        }
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let links = <LinearClient as IssueTracker>::get_issue_links(&client, "ORE-2").unwrap();

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, "relation-1");
        assert_eq!(links[0].link_type.id, "blocks");
        assert_eq!(links[0].direction.as_deref(), Some("INWARD"));
        assert_eq!(links[0].issues[0].id_readable.as_deref(), Some("ORE-1"));
    }

    #[tokio::test]
    async fn test_create_issue_matches_non_ascii_state_name_case_insensitively() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(body_string_contains("query TeamConnection"))
            .respond_with(ResponseTemplate::new(200).set_body_json(team_connection_page(
                vec![
                    serde_json::json!({ "id": "state-open", "name": "Geöffnet", "type": "unstarted", "position": 1.0 }),
                    serde_json::json!({ "id": "state-done", "name": "Erledigt", "type": "completed", "position": 2.0 }),
                ],
                None,
            )))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_string_contains("mutation IssueCreate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issueCreate": {
                        "success": true,
                        "issue": mock_linear_issue("ORE-9", "New issue")
                    }
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let create = CreateIssue {
            project_id: "team-ore".to_string(),
            summary: "New issue".to_string(),
            description: None,
            custom_fields: vec![CustomFieldUpdate::State {
                name: "Status".to_string(),
                value: "GEÖFFNET".to_string(),
            }],
            tags: vec![],
            parent: None,
        };
        <LinearClient as IssueTracker>::create_issue(&client, &create).unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        let create_request = requests
            .iter()
            .find(|request| String::from_utf8_lossy(&request.body).contains("mutation IssueCreate"))
            .unwrap();
        assert_eq!(
            request_variables(create_request)["input"]["stateId"],
            "state-open"
        );
    }

    // ==================== Error and HTTP Plumbing ====================
    //
    // These pin how failures surface through the IssueTracker trait.

    /// Unwrap a `TrackerError::Api` into its status and message.
    #[track_caller]
    fn api_error(err: TrackerError) -> (u16, String) {
        match err {
            TrackerError::Api { status, message } => (status, message),
            other => panic!("expected Api error, got: {other:?}"),
        }
    }

    /// The error `IssueTracker::get_issue` reports when every request is
    /// answered with `response`.
    async fn trait_get_issue_error(response: ResponseTemplate) -> TrackerError {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(response)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        <LinearClient as IssueTracker>::get_issue(&client, "ORE-1").unwrap_err()
    }

    /// A `query Teams` response holding every team in one page.
    fn teams_page(nodes: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "data": {
                "teams": {
                    "nodes": nodes,
                    "pageInfo": { "hasNextPage": false, "endCursor": null }
                }
            }
        })
    }

    /// Mount the teams-list query `get_team` falls back to, expected to be
    /// requested `times` times. `query Team(` (the by-id lookup) does not
    /// match it.
    async fn mount_list_teams(mock_server: &MockServer, times: u64) {
        Mock::given(method("POST"))
            .and(body_string_contains("query Teams"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(teams_page(vec![mock_team("ORE"), mock_team("ENG")])),
            )
            .expect(times)
            .mount(mock_server)
            .await;
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
    async fn test_http_429_is_api_429_rate_limit() {
        let err =
            trait_get_issue_error(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "errors": [{ "message": "slow" }]
            })))
            .await;

        // The body's message is dropped for a fixed rate-limit message.
        assert_eq!(
            api_error(err),
            (429, "Linear API rate limit exceeded".to_string())
        );
    }

    #[tokio::test]
    async fn test_non_2xx_graphql_errors_joined() {
        let err =
            trait_get_issue_error(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errors": [{ "message": "a" }, { "x": 1 }, { "message": "b" }]
            })))
            .await;

        // Errors without a message are skipped.
        assert_eq!(api_error(err), (400, "a; b".to_string()));
    }

    #[tokio::test]
    async fn test_non_2xx_top_level_message_and_empty_message() {
        let cases = [
            (
                ResponseTemplate::new(400).set_body_json(serde_json::json!({ "message": "m" })),
                400,
                "m",
            ),
            (
                ResponseTemplate::new(400).set_body_json(serde_json::json!({ "message": "" })),
                400,
                "",
            ),
            (ResponseTemplate::new(500), 500, "HTTP 500"),
            (
                ResponseTemplate::new(503).set_body_string("down"),
                503,
                "down",
            ),
        ];

        for (response, status, message) in cases {
            let err = trait_get_issue_error(response).await;
            assert_eq!(api_error(err), (status, message.to_string()));
        }
    }

    #[tokio::test]
    async fn test_non_2xx_error_message_fallbacks() {
        let cases = [
            // Errors without any message fall back to the top-level message.
            (400, r#"{"errors":[{"x":1}],"message":"m"}"#, "m"),
            // Error messages win over the top-level message.
            (400, r#"{"errors":[{"message":"a"}],"message":"m"}"#, "a"),
            // No string message anywhere: the body verbatim.
            (400, r#"{"errors":[{"x":1}]}"#, r#"{"errors":[{"x":1}]}"#),
            (400, r#"{"message":5}"#, r#"{"message":5}"#),
            // Only 401 and 429 are special-cased.
            (403, r#"{"message":"forbidden"}"#, "forbidden"),
            (404, r#"{"message":"missing"}"#, "missing"),
        ];

        for (status, body, message) in cases {
            let err =
                trait_get_issue_error(ResponseTemplate::new(status).set_body_string(body)).await;
            assert_eq!(
                api_error(err),
                (status, message.to_string()),
                "body: {body}"
            );
        }
    }

    #[tokio::test]
    async fn test_ratelimited_code_on_non_2xx_is_api() {
        let err =
            trait_get_issue_error(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errors": [{ "message": "limited", "extensions": { "code": "RATELIMITED" } }]
            })))
            .await;

        // The RATELIMITED code is only looked at on a 2xx response.
        assert_eq!(api_error(err), (400, "limited".to_string()));
    }

    #[tokio::test]
    async fn test_unauthorized_ignores_body_via_trait() {
        for response in [
            ResponseTemplate::new(401).set_body_string("nope"),
            ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "errors": [{ "message": "limited", "extensions": { "code": "RATELIMITED" } }]
            })),
        ] {
            let err = trait_get_issue_error(response).await;
            assert!(
                matches!(err, TrackerError::Unauthorized),
                "expected Unauthorized, got: {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_graphql_ratelimited_code_on_200_is_api_429() {
        let err =
            trait_get_issue_error(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "errors": [
                    { "message": "other" },
                    { "message": "Too many", "extensions": { "code": "ratelimited" } }
                ]
            })))
            .await;

        // Any error carrying the code counts, compared case-insensitively.
        assert_eq!(
            api_error(err),
            (429, "Linear API rate limit exceeded".to_string())
        );
    }

    #[tokio::test]
    async fn test_graphql_errors_on_200_are_api_200_joined() {
        let err =
            trait_get_issue_error(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "issue": mock_linear_issue("ORE-1", "Partial") },
                "errors": [{ "message": "a" }, { "message": "b" }]
            })))
            .await;

        // Errors win even when data came back too.
        assert_eq!(api_error(err), (200, "a; b".to_string()));
    }

    #[tokio::test]
    async fn test_graphql_error_without_message_on_200_is_parse() {
        let err =
            trait_get_issue_error(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "errors": [{ "extensions": { "code": "INTERNAL" } }]
            })))
            .await;

        // Unlike the non-2xx path, a 2xx envelope requires every error to have
        // a message, so this does not decode.
        assert!(
            matches!(err, TrackerError::Parse(_)),
            "expected Parse, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_empty_graphql_errors_array_is_ignored() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "issue": mock_linear_issue("ORE-1", "Found") },
                "errors": []
            })))
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let issue = <LinearClient as IssueTracker>::get_issue(&client, "ORE-1").unwrap();

        assert_eq!(issue.id_readable, "ORE-1");
    }

    #[tokio::test]
    async fn test_non_json_success_body_is_parse() {
        let err =
            trait_get_issue_error(ResponseTemplate::new(200).set_body_string("not-json")).await;

        // A 2xx body that is not the expected JSON is Parse, as in every
        // backend; a failed read stays Http.
        assert!(
            matches!(err, TrackerError::Parse(_)),
            "expected Parse, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_missing_data_is_api_200() {
        for body in [serde_json::json!({}), serde_json::json!({ "data": null })] {
            let err =
                trait_get_issue_error(ResponseTemplate::new(200).set_body_json(body.clone())).await;
            assert_eq!(
                api_error(err),
                (
                    200,
                    "Linear GraphQL response did not contain data".to_string()
                ),
                "body: {body}"
            );
        }
    }

    #[tokio::test]
    async fn test_non_2xx_unreadable_body_is_http_status() {
        // The body is cut off before its Content-Length; the failed read
        // counts as an empty body.
        let uri = serve_raw_response_once(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{\"message\":",
        );

        let client = LinearClient::with_base_url(&uri, "test-token");
        let err = <LinearClient as IssueTracker>::get_issue(&client, "ORE-1").unwrap_err();

        assert_eq!(api_error(err), (500, "HTTP 500".to_string()));
    }

    #[tokio::test]
    async fn test_success_body_cut_off_is_http() {
        // A 2xx body that cannot be read in full is a transport failure, not
        // JSON that failed to parse.
        let uri = serve_raw_response_once(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{\"data\":",
        );

        let client = LinearClient::with_base_url(&uri, "test-token");
        let err = <LinearClient as IssueTracker>::get_issue(&client, "ORE-1").unwrap_err();

        assert!(
            matches!(err, TrackerError::Http(_)),
            "expected Http, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_connection_failure_is_http() {
        // Bind and drop a listener so nothing accepts on the port.
        let addr = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();

        let client = LinearClient::with_base_url(&format!("http://{addr}"), "test-token");
        let err = <LinearClient as IssueTracker>::get_issue(&client, "ORE-1").unwrap_err();

        assert!(
            matches!(err, TrackerError::Http(_)),
            "expected Http, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_get_team_rate_limit_propagates_without_list_teams() {
        let mock_server = MockServer::start().await;
        mount_list_teams(&mock_server, 0).await;

        Mock::given(method("POST"))
            .and(body_string_contains("query Team("))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "errors": [{ "message": "slow" }]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let err = <LinearClient as IssueTracker>::get_project(&client, "ENG").unwrap_err();

        assert_eq!(
            api_error(err),
            (429, "Linear API rate limit exceeded".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_team_other_errors_propagate_without_list_teams() {
        type IsExpected = fn(&TrackerError) -> bool;

        let cases: [(&str, ResponseTemplate, IsExpected); 3] = [
            ("401", ResponseTemplate::new(401), |err| {
                matches!(err, TrackerError::Unauthorized)
            }),
            (
                "RATELIMITED code",
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "errors": [{ "message": "slow", "extensions": { "code": "RATELIMITED" } }]
                })),
                |err| {
                    matches!(
                        err,
                        TrackerError::Api { status: 429, message }
                            if message == "Linear API rate limit exceeded"
                    )
                },
            ),
            (
                "non-JSON body",
                ResponseTemplate::new(200).set_body_string("not-json"),
                |err| matches!(err, TrackerError::Parse(_)),
            ),
        ];

        for (case, response, expected) in cases {
            let mock_server = MockServer::start().await;
            mount_list_teams(&mock_server, 0).await;

            Mock::given(method("POST"))
                .and(body_string_contains("query Team("))
                .respond_with(response)
                .expect(1)
                .mount(&mock_server)
                .await;

            let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
            let err = <LinearClient as IssueTracker>::get_project(&client, "ENG").unwrap_err();

            assert!(expected(&err), "{case}: unexpected error: {err:?}");
        }
    }

    #[tokio::test]
    async fn test_get_team_api_error_falls_back_to_list_teams() {
        let mock_server = MockServer::start().await;
        mount_list_teams(&mock_server, 1).await;

        Mock::given(method("POST"))
            .and(body_string_contains("query Team("))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "errors": [{ "message": "Entity not found" }]
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let project = <LinearClient as IssueTracker>::get_project(&client, "ENG").unwrap();

        assert_eq!(project.id, "team-eng");
        assert_eq!(project.short_name, "ENG");
    }

    #[tokio::test]
    async fn test_get_team_null_team_falls_back_to_list_teams() {
        let mock_server = MockServer::start().await;
        mount_list_teams(&mock_server, 1).await;

        Mock::given(method("POST"))
            .and(body_string_contains("query Team("))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "team": null }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = LinearClient::with_base_url(&mock_server.uri(), "test-token");
        let err = <LinearClient as IssueTracker>::get_project(&client, "OPS").unwrap_err();

        // No listed team matches either, so it is reported as not found.
        assert!(
            matches!(err, TrackerError::ProjectNotFound(ref id) if id == "OPS"),
            "expected ProjectNotFound(\"OPS\"), got: {err:?}"
        );
    }
}
