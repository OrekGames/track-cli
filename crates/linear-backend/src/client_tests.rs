//! Unit tests for LinearClient using wiremock.

#[cfg(test)]
mod tests {
    use crate::client::LinearClient;
    use crate::error::LinearError;
    use tracker_core::IssueTracker;
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
