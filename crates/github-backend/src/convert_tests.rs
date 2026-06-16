#[cfg(test)]
mod tests {
    use crate::convert::{convert_query_to_github, github_timeline_to_events};
    use crate::models::GitHubTimelineEvent;

    /// Build a typed timeline event from raw JSON, mirroring how the client
    /// deserializes each element of the timeline array.
    fn event_from_json(value: serde_json::Value) -> GitHubTimelineEvent {
        GitHubTimelineEvent::from_value(value).unwrap()
    }

    #[test]
    fn github_timeline_derives_status_from_running_state() {
        // A close/reopen/close sequence (oldest-first). Because GitHub never
        // sends a `from` for status, we reconstruct it by threading a running
        // status seeded to "open".
        let events = vec![
            event_from_json(serde_json::json!({
                "event": "closed",
                "created_at": "2024-01-01T10:00:00Z",
                "actor": { "login": "alice", "id": 1 }
            })),
            event_from_json(serde_json::json!({
                "event": "reopened",
                "created_at": "2024-01-02T10:00:00Z",
                "actor": { "login": "bob", "id": 2 }
            })),
            event_from_json(serde_json::json!({
                "event": "closed",
                "created_at": "2024-01-03T10:00:00Z",
                "actor": { "login": "carol", "id": 3 }
            })),
        ];

        let out = github_timeline_to_events(events);

        assert_eq!(out.len(), 3);
        // All three are status transitions, folded onto the canonical name.
        assert!(out.iter().all(|e| e.field == "status"));

        // Output is newest-first, so display order reverses the chronological
        // from-derivation walk:
        //   chronological: open->closed, closed->open, open->closed
        //   newest-first:  open->closed (3rd), closed->open (2nd), open->closed (1st)
        assert_eq!(out[0].from.as_deref(), Some("open"));
        assert_eq!(out[0].to.as_deref(), Some("closed"));
        assert_eq!(out[0].author.as_ref().unwrap().login, "carol");

        assert_eq!(out[1].from.as_deref(), Some("closed"));
        assert_eq!(out[1].to.as_deref(), Some("open"));
        assert_eq!(out[1].author.as_ref().unwrap().login, "bob");

        assert_eq!(out[2].from.as_deref(), Some("open"));
        assert_eq!(out[2].to.as_deref(), Some("closed"));
        assert_eq!(out[2].author.as_ref().unwrap().login, "alice");
    }

    #[test]
    fn github_timeline_mixed_assign_label_rename() {
        // Non-status fields follow the status-only-from policy: assignee/label
        // carry `from: None` (assigned) / `to: None` (only on un* events), while
        // `renamed` is the one event that carries a real from/to.
        let events = vec![
            event_from_json(serde_json::json!({
                "event": "assigned",
                "created_at": "2024-01-01T10:00:00Z",
                "actor": { "login": "alice", "id": 1 },
                "assignee": { "login": "dave", "id": 4 }
            })),
            event_from_json(serde_json::json!({
                "event": "labeled",
                "created_at": "2024-01-02T10:00:00Z",
                "actor": { "login": "alice", "id": 1 },
                "label": { "id": 9, "name": "bug", "color": "fc2929", "description": null }
            })),
            event_from_json(serde_json::json!({
                "event": "renamed",
                "created_at": "2024-01-03T10:00:00Z",
                "actor": { "login": "alice", "id": 1 },
                "rename": { "from": "Old title", "to": "New title" }
            })),
        ];

        let out = github_timeline_to_events(events);
        assert_eq!(out.len(), 3);

        // Newest-first: renamed, labeled, assigned.
        let renamed = &out[0];
        assert_eq!(renamed.field, "title");
        assert_eq!(renamed.from.as_deref(), Some("Old title"));
        assert_eq!(renamed.to.as_deref(), Some("New title"));

        let labeled = &out[1];
        assert_eq!(labeled.field, "labels");
        assert_eq!(labeled.from, None);
        assert_eq!(labeled.to.as_deref(), Some("bug"));

        let assigned = &out[2];
        assert_eq!(assigned.field, "assignee");
        assert_eq!(assigned.from, None);
        assert_eq!(assigned.to.as_deref(), Some("dave"));
    }

    #[test]
    fn github_timeline_handles_null_actor() {
        // A system close (no actor) yields a status event with author None.
        let events = vec![event_from_json(serde_json::json!({
            "event": "closed",
            "created_at": "2024-01-01T10:00:00Z",
            "actor": null
        }))];

        let out = github_timeline_to_events(events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].field, "status");
        assert_eq!(out[0].from.as_deref(), Some("open"));
        assert_eq!(out[0].to.as_deref(), Some("closed"));
        assert!(out[0].author.is_none());
    }

    #[test]
    fn github_timeline_ignores_unknown_events() {
        // commented/referenced/etc. map to `Other` and are dropped; surrounding
        // status events still derive correctly and unparseable dates are skipped.
        let events = vec![
            event_from_json(serde_json::json!({
                "event": "commented",
                "created_at": "2024-01-01T10:00:00Z",
                "actor": { "login": "alice", "id": 1 }
            })),
            event_from_json(serde_json::json!({
                "event": "referenced",
                "commit_id": "abc123"
            })),
            event_from_json(serde_json::json!({
                "event": "closed",
                "created_at": "not-a-date",
                "actor": { "login": "bob", "id": 2 }
            })),
            event_from_json(serde_json::json!({
                "event": "closed",
                "created_at": "2024-01-02T10:00:00Z",
                "actor": { "login": "carol", "id": 3 }
            })),
        ];

        let out = github_timeline_to_events(events);
        // Only the one parseable close survives.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].field, "status");
        // The unparseable-date close did NOT advance the running status, so the
        // surviving close still reads from "open".
        assert_eq!(out[0].from.as_deref(), Some("open"));
        assert_eq!(out[0].to.as_deref(), Some("closed"));
        assert_eq!(out[0].author.as_ref().unwrap().login, "carol");
    }

    #[test]
    fn test_convert_query_to_github() {
        let cases = vec![
            ("", "is:issue"),
            ("   ", "is:issue"),
            ("bug", "bug is:issue"),
            ("project:owner/repo", "is:issue"),
            ("project:owner/repo bug", "bug is:issue"),
            ("#open", "is:open is:issue"),
            ("#unresolved", "is:open is:issue"),
            ("#closed", "is:closed is:issue"),
            ("#resolved", "is:closed is:issue"),
            ("#bug", "label:bug is:issue"),
            ("bug #open", "bug is:open is:issue"),
            (
                "some term #feature #open",
                "some term label:feature is:open is:issue",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(
                convert_query_to_github(input),
                expected,
                "Failed for input: '{}'",
                input
            );
        }
    }
}
