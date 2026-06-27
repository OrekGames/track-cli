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

    // ------------------------------------------------------------------
    // Lossless custom-field projection (issue #277)
    // ------------------------------------------------------------------

    use crate::convert::github_issue_to_core;
    use crate::models::GitHubIssue;
    use tracker_core::CustomField;

    /// Build a GitHubIssue from a JSON object that supplies the required base
    /// fields plus whatever extra keys the test exercises. `#[serde(flatten)]`
    /// routes the extras into `issue.extra`.
    fn issue_from_json(extra: serde_json::Value) -> GitHubIssue {
        let mut base = serde_json::json!({
            "id": 1,
            "number": 42,
            "title": "Example issue",
            "body": null,
            "state": "open",
            "created_at": "2024-01-01T10:00:00Z",
            "updated_at": "2024-01-02T10:00:00Z",
            "closed_at": null
        });
        let map = base.as_object_mut().unwrap();
        for (k, v) in extra.as_object().unwrap() {
            map.insert(k.clone(), v.clone());
        }
        serde_json::from_value(base).unwrap()
    }

    /// Find custom fields surfaced under `name` (case-sensitive).
    fn fields_named<'a>(fields: &'a [CustomField], name: &str) -> Vec<&'a CustomField> {
        fields
            .iter()
            .filter(|cf| match cf {
                CustomField::SingleEnum { name: n, .. }
                | CustomField::State { name: n, .. }
                | CustomField::SingleUser { name: n, .. }
                | CustomField::Text { name: n, .. }
                | CustomField::MultiEnum { name: n, .. }
                | CustomField::Unknown { name: n, .. } => n == name,
            })
            .collect()
    }

    #[test]
    fn projection_surfaces_scalar_as_text() {
        let issue = issue_from_json(serde_json::json!({ "state_reason": "completed" }));
        let core = github_issue_to_core(issue, "owner", "repo");

        let matches = fields_named(&core.custom_fields, "state_reason");
        assert_eq!(matches.len(), 1);
        match matches[0] {
            CustomField::Text { value, .. } => assert_eq!(value.as_deref(), Some("completed")),
            other => panic!("expected Text, got {:?}", other),
        }
    }

    #[test]
    fn projection_strips_trailing_zero_from_numbers() {
        let issue = issue_from_json(serde_json::json!({ "weight": 3.0, "count": 7 }));
        let core = github_issue_to_core(issue, "owner", "repo");

        match fields_named(&core.custom_fields, "weight")[0] {
            CustomField::Text { value, .. } => assert_eq!(value.as_deref(), Some("3")),
            other => panic!("expected Text, got {:?}", other),
        }
        match fields_named(&core.custom_fields, "count")[0] {
            CustomField::Text { value, .. } => assert_eq!(value.as_deref(), Some("7")),
            other => panic!("expected Text, got {:?}", other),
        }
    }

    #[test]
    fn projection_preserves_large_unsigned_numbers() {
        let issue = issue_from_json(serde_json::json!({ "database_id": u64::MAX }));
        let core = github_issue_to_core(issue, "owner", "repo");

        match fields_named(&core.custom_fields, "database_id")[0] {
            CustomField::Text { value, .. } => {
                assert_eq!(value.as_deref(), Some("18446744073709551615"));
            }
            other => panic!("expected Text, got {:?}", other),
        }
    }

    #[test]
    fn projection_drops_noise_keys() {
        let issue = issue_from_json(serde_json::json!({
            "html_url": "https://github.com/owner/repo/issues/42",
            "node_id": "I_abc123",
            "reactions": { "total_count": 3, "+1": 2 },
            "author_association": "OWNER"
        }));
        let core = github_issue_to_core(issue, "owner", "repo");

        for noise in ["html_url", "node_id", "reactions", "author_association"] {
            assert!(
                fields_named(&core.custom_fields, noise).is_empty(),
                "noise key {} leaked into projection",
                noise
            );
        }
    }

    #[test]
    fn projection_does_not_duplicate_typed_fields() {
        // Real state/assignee/milestone live on named fields and are surfaced by
        // the hardcoded pushes. They must NOT also appear via `extra`, proving
        // serde flatten never re-captures a matched key.
        let issue = issue_from_json(serde_json::json!({
            "state": "closed",
            "assignee": { "login": "alice", "id": 7 },
            "milestone": { "id": 1, "number": 1, "title": "v1.0" }
        }));
        let core = github_issue_to_core(issue, "owner", "repo");

        // Exactly one Status (from named `state`), is_resolved true.
        let status = fields_named(&core.custom_fields, "Status");
        assert_eq!(status.len(), 1);
        match status[0] {
            CustomField::State {
                value, is_resolved, ..
            } => {
                assert_eq!(value.as_deref(), Some("closed"));
                assert!(is_resolved);
            }
            other => panic!("expected State, got {:?}", other),
        }

        // Exactly one Assignee SingleUser from the named field.
        let assignee = fields_named(&core.custom_fields, "Assignee");
        assert_eq!(assignee.len(), 1);
        match assignee[0] {
            CustomField::SingleUser { login, .. } => assert_eq!(login.as_deref(), Some("alice")),
            other => panic!("expected SingleUser, got {:?}", other),
        }

        // Exactly one Milestone SingleEnum from the named field.
        let milestone = fields_named(&core.custom_fields, "Milestone");
        assert_eq!(milestone.len(), 1);
        match milestone[0] {
            CustomField::SingleEnum { value, .. } => assert_eq!(value.as_deref(), Some("v1.0")),
            other => panic!("expected SingleEnum, got {:?}", other),
        }

        // No lowercase leak from extra (proves no duplication).
        assert!(fields_named(&core.custom_fields, "state").is_empty());
        assert!(fields_named(&core.custom_fields, "assignee").is_empty());
        assert!(fields_named(&core.custom_fields, "milestone").is_empty());
    }

    #[test]
    fn projection_surfaces_user_object_as_single_user() {
        let issue = issue_from_json(serde_json::json!({
            "closed_by": { "login": "alice", "id": 7 }
        }));
        let core = github_issue_to_core(issue, "owner", "repo");

        let matches = fields_named(&core.custom_fields, "closed_by");
        assert_eq!(matches.len(), 1);
        match matches[0] {
            CustomField::SingleUser {
                login,
                display_name,
                ..
            } => {
                assert_eq!(login.as_deref(), Some("alice"));
                // No `name` on the object, so display falls back to login.
                assert_eq!(display_name.as_deref(), Some("alice"));
            }
            other => panic!("expected SingleUser, got {:?}", other),
        }
    }

    #[test]
    fn projection_surfaces_named_user_consumed_before_extra() {
        let issue = issue_from_json(serde_json::json!({
            "user": {
                "login": "reporter",
                "id": 9,
                "avatar_url": "https://avatars.example/reporter"
            }
        }));
        assert!(
            !issue.extra.contains_key("user"),
            "named user should be consumed before flatten extra"
        );

        let core = github_issue_to_core(issue, "owner", "repo");

        let matches = fields_named(&core.custom_fields, "user");
        assert_eq!(matches.len(), 1);
        match matches[0] {
            CustomField::SingleUser {
                login,
                display_name,
                ..
            } => {
                assert_eq!(login.as_deref(), Some("reporter"));
                assert_eq!(display_name.as_deref(), Some("reporter"));
            }
            other => panic!("expected SingleUser, got {:?}", other),
        }
    }

    #[test]
    fn projection_preserves_named_assignees_without_replacing_assignee() {
        let assignees = serde_json::json!([
            {
                "login": "alice",
                "id": 7,
                "avatar_url": "https://avatars.example/alice",
                "type": "User"
            },
            {
                "login": "bob",
                "id": 8,
                "avatar_url": "https://avatars.example/bob",
                "type": "User"
            }
        ]);
        let issue = issue_from_json(serde_json::json!({
            "assignee": { "login": "alice", "id": 7 },
            "assignees": assignees.clone()
        }));
        assert!(
            !issue.extra.contains_key("assignees"),
            "named assignees should be consumed before flatten extra"
        );

        let core = github_issue_to_core(issue, "owner", "repo");

        let assignee = fields_named(&core.custom_fields, "Assignee");
        assert_eq!(assignee.len(), 1);
        match assignee[0] {
            CustomField::SingleUser { login, .. } => assert_eq!(login.as_deref(), Some("alice")),
            other => panic!("expected SingleUser, got {:?}", other),
        }

        let matches = fields_named(&core.custom_fields, "assignees");
        assert_eq!(matches.len(), 1);
        match matches[0] {
            CustomField::Unknown { value, .. } => {
                assert_eq!(value.as_ref(), Some(&assignees));
            }
            other => panic!("expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn projection_array_of_strings_is_multi_enum() {
        let issue = issue_from_json(serde_json::json!({
            "topics": ["alpha", "beta", "gamma"]
        }));
        let core = github_issue_to_core(issue, "owner", "repo");

        let matches = fields_named(&core.custom_fields, "topics");
        assert_eq!(matches.len(), 1);
        match matches[0] {
            CustomField::MultiEnum { values, .. } => {
                assert_eq!(values, &vec!["alpha", "beta", "gamma"]);
            }
            other => panic!("expected MultiEnum, got {:?}", other),
        }
    }

    #[test]
    fn projection_array_of_rich_objects_is_unknown_whole() {
        let arr = serde_json::json!([
            { "login": "alice", "id": 7, "type": "User" },
            { "login": "bob", "id": 8, "type": "User" }
        ]);
        let issue = issue_from_json(serde_json::json!({ "requested_reviewers": arr.clone() }));
        let core = github_issue_to_core(issue, "owner", "repo");

        let matches = fields_named(&core.custom_fields, "requested_reviewers");
        assert_eq!(matches.len(), 1);
        match matches[0] {
            CustomField::Unknown { value, .. } => {
                assert_eq!(value.as_ref(), Some(&arr));
            }
            other => panic!("expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn projection_skips_null_extra() {
        let issue = issue_from_json(serde_json::json!({ "state_reason": null }));
        let core = github_issue_to_core(issue, "owner", "repo");

        assert!(
            fields_named(&core.custom_fields, "state_reason").is_empty(),
            "null extra should not be surfaced"
        );
    }
}
