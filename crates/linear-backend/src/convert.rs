//! Model conversions from Linear types to tracker-core types.

use serde_json::{Value, json};
use tracker_core::{
    Comment, CommentAuthor, CustomField, Issue, IssueHistoryEvent, IssueLink, IssueLinkType,
    IssueTag, LinkedIssue, Project, ProjectCustomField, ProjectRef, StateValueInfo, Tag, TagColor,
    User, canonical_field_name,
};

use crate::client::{LinearIssueRelations, add_filter_condition, filter_with_team};
use crate::models::*;

#[derive(Debug, Clone, Default)]
pub struct ParsedLinearQuery {
    pub team: Option<String>,
    pub state: Option<String>,
    pub state_type_in: Option<Vec<String>>,
    pub state_type_nin: Option<Vec<String>>,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub priority: Option<i64>,
    pub text: Vec<String>,
}

pub fn linear_issue_to_core(issue: LinearIssue) -> Issue {
    let state_name = issue.state.as_ref().map(|state| state.name.clone());
    let is_resolved = issue
        .state
        .as_ref()
        .is_some_and(|state| state.state_type == "completed" || state.state_type == "canceled");

    let mut custom_fields = Vec::new();
    custom_fields.push(CustomField::State {
        name: "Status".to_string(),
        value: state_name,
        is_resolved,
    });
    custom_fields.push(CustomField::SingleUser {
        name: "Assignee".to_string(),
        login: issue.assignee.as_ref().map(linear_user_login),
        display_name: issue.assignee.as_ref().map(linear_user_display_name),
    });
    custom_fields.push(CustomField::SingleEnum {
        name: "Priority".to_string(),
        value: Some(
            issue
                .priority_label
                .unwrap_or_else(|| priority_label(issue.priority).to_string()),
        ),
    });

    if let Some(project) = &issue.project {
        custom_fields.push(CustomField::SingleEnum {
            name: "Project".to_string(),
            value: Some(project.name.clone()),
        });
    }

    if let Some(parent) = &issue.parent {
        custom_fields.push(CustomField::SingleEnum {
            name: "Parent".to_string(),
            value: Some(parent.identifier.clone()),
        });
    }

    if let Some(estimate) = issue.estimate {
        custom_fields.push(CustomField::Text {
            name: "Estimate".to_string(),
            value: Some(format_estimate(estimate)),
        });
    }

    if let Some(due_date) = &issue.due_date {
        custom_fields.push(CustomField::Text {
            name: "Due Date".to_string(),
            value: Some(due_date.clone()),
        });
    }

    if let Some(cycle) = &issue.cycle {
        let value = cycle.name.as_ref().cloned().unwrap_or_else(|| {
            let label = cycle
                .number
                .map(format_estimate)
                .unwrap_or_else(|| cycle.id.clone());
            format!("Cycle {label}")
        });
        custom_fields.push(CustomField::SingleEnum {
            name: "Cycle".to_string(),
            value: Some(value),
        });
    }

    if let Some(creator) = &issue.creator {
        custom_fields.push(CustomField::SingleUser {
            name: "Creator".to_string(),
            login: Some(linear_user_login(creator)),
            display_name: Some(linear_user_display_name(creator)),
        });
    }

    let tags = issue
        .labels
        .nodes
        .iter()
        .map(|label| Tag {
            id: label.id.clone(),
            name: label.name.clone(),
        })
        .collect();

    Issue {
        id: issue.id,
        id_readable: issue.identifier,
        summary: issue.title,
        description: issue.description.filter(|s| !s.is_empty()),
        project: ProjectRef {
            id: issue.team.id,
            name: Some(issue.team.name),
            short_name: Some(issue.team.key),
        },
        custom_fields,
        tags,
        created: issue.created_at,
        updated: issue.updated_at,
        resolved: issue.completed_at.or(issue.canceled_at),
    }
}

impl From<LinearTeam> for Project {
    fn from(team: LinearTeam) -> Self {
        Self {
            id: team.id,
            name: team.name,
            short_name: team.key,
            description: team.description,
        }
    }
}

impl From<LinearIssueLabel> for IssueTag {
    fn from(label: LinearIssueLabel) -> Self {
        Self {
            id: label.id.clone(),
            name: label.name,
            color: label.color.map(|color| TagColor {
                id: color.clone(),
                background: Some(color),
                foreground: None,
            }),
            issues_count: None,
        }
    }
}

impl From<LinearUser> for User {
    fn from(user: LinearUser) -> Self {
        Self {
            id: user.id,
            login: user.email.or(Some(user.name.clone())),
            display_name: user.display_name.unwrap_or(user.name),
        }
    }
}

impl From<LinearComment> for Comment {
    fn from(comment: LinearComment) -> Self {
        Self {
            id: comment.id,
            text: comment.body,
            author: comment.user.map(|user| CommentAuthor {
                login: linear_user_login(&user),
                name: Some(linear_user_display_name(&user)),
            }),
            created: Some(comment.created_at),
        }
    }
}

/// Convert Linear issue-history nodes into core history events, newest-first.
///
/// Linear models history as nodes that may each carry several independent
/// transitions (state, assignee, priority, title), so every populated
/// transition is **decomposed** into its own [`IssueHistoryEvent`], all sharing
/// the node's `createdAt` and `actor`. Nodes whose timestamp is missing are
/// skipped rather than fabricated. The state field is canonicalized so Linear's
/// workflow "state" folds onto the portable "status" token. `from`/`to` are
/// `None` when the corresponding source field is null.
///
/// Label history (`addedLabelIds`/`removedLabelIds`) is intentionally not
/// emitted here — see the TODO below.
pub fn linear_history_to_events(nodes: Vec<LinearIssueHistory>) -> Vec<IssueHistoryEvent> {
    let mut events = Vec::new();
    for node in nodes {
        // A node without a timestamp can't be placed on the timeline; skip it
        // rather than stamping a fabricated `at`.
        let Some(at) = node.created_at else {
            continue;
        };
        let author = node.actor.as_ref().map(|user| CommentAuthor {
            login: linear_user_login(user),
            name: Some(linear_user_display_name(user)),
        });

        // State transition -> canonical "status" (only when the value changed).
        let from_state = node.from_state.as_ref().map(|s| s.name.clone());
        let to_state = node.to_state.as_ref().map(|s| s.name.clone());
        if to_state.is_some() && to_state != from_state {
            events.push(IssueHistoryEvent {
                at,
                author: author.clone(),
                field: canonical_field_name("state"),
                from: from_state,
                to: to_state,
            });
        }

        // Assignee transition (only when something actually changed).
        let from_assignee = node.from_assignee.as_ref().map(linear_user_display_name);
        let to_assignee = node.to_assignee.as_ref().map(linear_user_display_name);
        if (from_assignee.is_some() || to_assignee.is_some()) && from_assignee != to_assignee {
            events.push(IssueHistoryEvent {
                at,
                author: author.clone(),
                field: "assignee".to_string(),
                from: from_assignee,
                to: to_assignee,
            });
        }

        // Priority transition (only when the value actually changed).
        let from_priority = node.from_priority;
        let to_priority = node.to_priority;
        if (from_priority.is_some() || to_priority.is_some()) && from_priority != to_priority {
            events.push(IssueHistoryEvent {
                at,
                author: author.clone(),
                field: "priority".to_string(),
                from: from_priority.map(|p| priority_label(p).to_string()),
                to: to_priority.map(|p| priority_label(p).to_string()),
            });
        }

        // Title transition (only when the value actually changed).
        let from_title = node.from_title.clone();
        let to_title = node.to_title.clone();
        if (from_title.is_some() || to_title.is_some()) && from_title != to_title {
            events.push(IssueHistoryEvent {
                at,
                author: author.clone(),
                field: "title".to_string(),
                from: from_title,
                to: to_title,
            });
        }

        // TODO: label history (addedLabelIds/removedLabelIds) is deferred —
        // those carry only label *ids*, which require a separate name lookup to
        // render portably. Documented follow-up.
    }

    // Linear returns history newest-first; re-sort defensively. `sort_by` is a
    // stable sort, so the per-node transition order (state, assignee, priority,
    // title) is preserved among events sharing a timestamp.
    events.sort_by(|a, b| b.at.cmp(&a.at));
    events
}

pub fn team_details_custom_fields(
    details: &crate::client::LinearTeamDetails,
) -> Vec<ProjectCustomField> {
    let mut states = details.states.nodes.clone();
    states.sort_by(|left, right| {
        left.position
            .partial_cmp(&right.position)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let state_values = states
        .iter()
        .enumerate()
        .map(|(idx, state)| StateValueInfo {
            name: state.name.clone(),
            is_resolved: state.state_type == "completed" || state.state_type == "canceled",
            ordinal: idx as i32,
        })
        .collect::<Vec<_>>();
    let state_names = states
        .iter()
        .map(|state| state.name.clone())
        .collect::<Vec<_>>();

    vec![
        ProjectCustomField {
            id: "status".to_string(),
            name: "Status".to_string(),
            field_type: "state[1]".to_string(),
            required: true,
            values: state_names,
            state_values,
        },
        ProjectCustomField {
            id: "assignee".to_string(),
            name: "Assignee".to_string(),
            field_type: "user[1]".to_string(),
            required: false,
            values: details
                .members
                .nodes
                .iter()
                .map(linear_user_login)
                .collect(),
            state_values: vec![],
        },
        ProjectCustomField {
            id: "priority".to_string(),
            name: "Priority".to_string(),
            field_type: "enum[1]".to_string(),
            required: false,
            values: vec![
                "No priority".to_string(),
                "Urgent".to_string(),
                "High".to_string(),
                "Medium".to_string(),
                "Low".to_string(),
            ],
            state_values: vec![],
        },
        ProjectCustomField {
            id: "labels".to_string(),
            name: "Labels".to_string(),
            field_type: "enum[*]".to_string(),
            required: false,
            values: details
                .labels
                .nodes
                .iter()
                .map(|label| label.name.clone())
                .collect(),
            state_values: vec![],
        },
        ProjectCustomField {
            id: "project".to_string(),
            name: "Project".to_string(),
            field_type: "enum[1]".to_string(),
            required: false,
            values: details
                .projects
                .nodes
                .iter()
                .map(|project| project.name.clone())
                .collect(),
            state_values: vec![],
        },
    ]
}

pub fn linear_link_types() -> Vec<IssueLinkType> {
    vec![
        IssueLinkType {
            id: "related".to_string(),
            name: "Relates".to_string(),
            source_to_target: Some("relates to".to_string()),
            target_to_source: Some("relates to".to_string()),
            directed: false,
        },
        IssueLinkType {
            id: "blocks".to_string(),
            name: "Blocks".to_string(),
            source_to_target: Some("blocks".to_string()),
            target_to_source: Some("is blocked by".to_string()),
            directed: true,
        },
        IssueLinkType {
            id: "duplicate".to_string(),
            name: "Duplicates".to_string(),
            source_to_target: Some("duplicates".to_string()),
            target_to_source: Some("is duplicated by".to_string()),
            directed: true,
        },
        IssueLinkType {
            id: "similar".to_string(),
            name: "Similar".to_string(),
            source_to_target: Some("is similar to".to_string()),
            target_to_source: Some("is similar to".to_string()),
            directed: false,
        },
    ]
}

pub fn linear_relations_to_core(
    current_issue_id: &str,
    relations: LinearIssueRelations,
) -> Vec<IssueLink> {
    let mut links = Vec::new();

    if let Some(parent) = relations.parent {
        links.push(IssueLink {
            id: format!("linear-parent:{}", parent.id),
            direction: Some("INWARD".to_string()),
            link_type: IssueLinkType {
                id: "parent".to_string(),
                name: "Parent".to_string(),
                source_to_target: Some("is parent of".to_string()),
                target_to_source: Some("is subtask of".to_string()),
                directed: true,
            },
            issues: vec![linked_issue(parent)],
        });
    }

    for child in relations.children {
        links.push(IssueLink {
            id: format!("linear-child:{}", child.id),
            direction: Some("OUTWARD".to_string()),
            link_type: IssueLinkType {
                id: "subtask".to_string(),
                name: "Subtask".to_string(),
                source_to_target: Some("is parent of".to_string()),
                target_to_source: Some("is subtask of".to_string()),
                directed: true,
            },
            issues: vec![linked_issue(child)],
        });
    }

    for relation in relations.relations {
        let is_source = relation.issue.id == current_issue_id;
        let other = if is_source {
            relation.related_issue
        } else {
            relation.issue
        };
        let (link_type, direction) = relation_link_type(&relation.relation_type, is_source);
        links.push(IssueLink {
            id: relation.id,
            direction: Some(direction.to_string()),
            link_type,
            issues: vec![linked_issue(other)],
        });
    }

    links
}

pub fn parse_linear_query(query: &str) -> ParsedLinearQuery {
    let tokens = tokenize_query(query);
    let mut parsed = ParsedLinearQuery::default();
    let mut idx = 0;

    while idx < tokens.len() {
        let token = &tokens[idx];
        if let Some(name) = token.strip_prefix('#') {
            match name.to_lowercase().as_str() {
                "unresolved" | "open" => {
                    parsed.state_type_nin =
                        Some(vec!["completed".to_string(), "canceled".to_string()]);
                }
                "resolved" | "closed" => {
                    parsed.state_type_in =
                        Some(vec!["completed".to_string(), "canceled".to_string()]);
                }
                other => parsed.labels.push(other.to_string()),
            }
            idx += 1;
            continue;
        }

        if let Some((key, value, consumed)) = parse_key_value(&tokens, idx) {
            match key.to_lowercase().as_str() {
                "project" | "team" => parsed.team = Some(value),
                "state" | "status" | "stage" => parsed.state = Some(value),
                "label" | "labels" | "tag" => parsed.labels.push(value),
                "assignee" => parsed.assignee = Some(value),
                "priority" => parsed.priority = parse_priority(&value),
                _ => parsed.text.push(format!("{key}:{value}")),
            }
            idx += consumed;
            continue;
        }

        parsed.text.push(token.clone());
        idx += 1;
    }

    parsed
}

pub fn build_filter_from_parsed(
    parsed: &ParsedLinearQuery,
    team_id: Option<&str>,
    assignee_id: Option<&str>,
) -> Option<Value> {
    let mut filter = filter_with_team(team_id);

    if let Some(state) = parsed.state.as_deref() {
        add_filter_condition(
            &mut filter,
            json!({
                "state": {
                    "name": { "eqIgnoreCase": state }
                }
            }),
        );
    }

    if let Some(types) = &parsed.state_type_in {
        add_filter_condition(
            &mut filter,
            json!({
                "state": {
                    "type": { "in": types }
                }
            }),
        );
    }

    if let Some(types) = &parsed.state_type_nin {
        add_filter_condition(
            &mut filter,
            json!({
                "state": {
                    "type": { "nin": types }
                }
            }),
        );
    }

    for label in &parsed.labels {
        add_filter_condition(
            &mut filter,
            json!({
                "labels": {
                    "name": { "eqIgnoreCase": label }
                }
            }),
        );
    }

    if let Some(id) = assignee_id {
        add_filter_condition(
            &mut filter,
            json!({
                "assignee": {
                    "id": { "eq": id }
                }
            }),
        );
    } else if let Some(assignee) = parsed.assignee.as_deref() {
        add_filter_condition(
            &mut filter,
            json!({
                "or": [
                    { "assignee": { "name": { "eqIgnoreCase": assignee } } },
                    { "assignee": { "displayName": { "eqIgnoreCase": assignee } } },
                    { "assignee": { "email": { "eqIgnoreCase": assignee } } }
                ]
            }),
        );
    }

    if let Some(priority) = parsed.priority {
        add_filter_condition(
            &mut filter,
            json!({
                "priority": { "eq": priority }
            }),
        );
    }

    filter
}

pub fn priority_from_label(value: &str) -> Option<i64> {
    parse_priority(value)
}

fn relation_link_type(
    relation_type: &str,
    current_is_source: bool,
) -> (IssueLinkType, &'static str) {
    match relation_type {
        "blocks" => (
            IssueLinkType {
                id: "blocks".to_string(),
                name: "Blocks".to_string(),
                source_to_target: Some("blocks".to_string()),
                target_to_source: Some("is blocked by".to_string()),
                directed: true,
            },
            if current_is_source {
                "OUTWARD"
            } else {
                "INWARD"
            },
        ),
        "duplicate" => (
            IssueLinkType {
                id: "duplicate".to_string(),
                name: "Duplicates".to_string(),
                source_to_target: Some("duplicates".to_string()),
                target_to_source: Some("is duplicated by".to_string()),
                directed: true,
            },
            if current_is_source {
                "OUTWARD"
            } else {
                "INWARD"
            },
        ),
        "similar" => (
            IssueLinkType {
                id: "similar".to_string(),
                name: "Similar".to_string(),
                source_to_target: Some("is similar to".to_string()),
                target_to_source: Some("is similar to".to_string()),
                directed: false,
            },
            "BOTH",
        ),
        _ => (
            IssueLinkType {
                id: "related".to_string(),
                name: "Relates".to_string(),
                source_to_target: Some("relates to".to_string()),
                target_to_source: Some("relates to".to_string()),
                directed: false,
            },
            "BOTH",
        ),
    }
}

fn linked_issue(issue: LinearIssueRef) -> LinkedIssue {
    LinkedIssue {
        id: issue.id,
        id_readable: Some(issue.identifier),
        summary: Some(issue.title),
    }
}

fn linear_user_login(user: &LinearUser) -> String {
    user.email.clone().unwrap_or_else(|| user.name.clone())
}

fn linear_user_display_name(user: &LinearUser) -> String {
    user.display_name
        .clone()
        .unwrap_or_else(|| user.name.clone())
}

/// Render a Linear numeric estimate/cycle number, stripping a trailing `.0` so
/// whole-number values read as `2` rather than `2.0` while fractional values
/// (e.g. `1.5`) keep their decimal part.
fn format_estimate(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn priority_label(priority: i64) -> &'static str {
    match priority {
        1 => "Urgent",
        2 => "High",
        3 => "Medium",
        4 => "Low",
        _ => "No priority",
    }
}

fn parse_priority(value: &str) -> Option<i64> {
    match value.trim().to_lowercase().as_str() {
        "0" | "none" | "no priority" => Some(0),
        "1" | "urgent" => Some(1),
        "2" | "high" => Some(2),
        "3" | "medium" | "normal" => Some(3),
        "4" | "low" => Some(4),
        _ => None,
    }
}

fn parse_key_value(tokens: &[String], idx: usize) -> Option<(String, String, usize)> {
    let token = &tokens[idx];
    if let Some((key, value)) = token.split_once(':')
        && !key.is_empty()
    {
        if !value.is_empty() {
            return Some((key.to_string(), value.to_string(), 1));
        }
        if let Some(next) = tokens.get(idx + 1) {
            return Some((key.to_string(), next.clone(), 2));
        }
    }
    None
}

fn tokenize_query(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut brace_depth = 0usize;

    for ch in query.chars() {
        match ch {
            '"' | '\'' if brace_depth == 0 => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                } else {
                    current.push(ch);
                }
            }
            '{' if quote.is_none() => {
                brace_depth += 1;
                if brace_depth > 1 {
                    current.push(ch);
                }
            }
            '}' if quote.is_none() && brace_depth > 0 => {
                brace_depth -= 1;
                if brace_depth > 0 {
                    current.push(ch);
                }
            }
            c if c.is_whitespace() && quote.is_none() && brace_depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_from_json(value: serde_json::Value) -> Vec<LinearIssueHistory> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn linear_history_decomposes_multi_change_node() {
        // One node carrying state + assignee + priority transitions must yield
        // three events, all sharing the node's timestamp and actor, in
        // per-node order (state, assignee, priority).
        let nodes = history_from_json(serde_json::json!([
            {
                "createdAt": "2024-01-15T10:00:00Z",
                "actor": { "id": "u-alice", "name": "alice", "displayName": "Alice", "email": "alice@example.com" },
                "fromState": { "name": "Todo" },
                "toState": { "name": "In Progress" },
                "fromAssignee": null,
                "toAssignee": { "id": "u-bob", "name": "bob", "displayName": "Bob", "email": "bob@example.com" },
                "fromPriority": 3,
                "toPriority": 1,
                "fromTitle": null,
                "toTitle": null
            }
        ]));

        let events = linear_history_to_events(nodes);
        assert_eq!(events.len(), 3);

        // State -> canonical "status".
        assert_eq!(events[0].field, "status");
        assert_eq!(events[0].from.as_deref(), Some("Todo"));
        assert_eq!(events[0].to.as_deref(), Some("In Progress"));

        // Assignee uses display name; a null `from` side maps to None.
        assert_eq!(events[1].field, "assignee");
        assert_eq!(events[1].from, None);
        assert_eq!(events[1].to.as_deref(), Some("Bob"));

        // Priority maps numeric codes through priority_label.
        assert_eq!(events[2].field, "priority");
        assert_eq!(events[2].from.as_deref(), Some("Medium"));
        assert_eq!(events[2].to.as_deref(), Some("Urgent"));

        // Author is shared across all three events.
        for event in &events {
            assert_eq!(
                event.author.as_ref().map(|a| a.login.as_str()),
                Some("alice@example.com")
            );
            assert_eq!(
                event.author.as_ref().and_then(|a| a.name.as_deref()),
                Some("Alice")
            );
        }
    }

    #[test]
    fn linear_history_null_actor_yields_no_author() {
        // A system change carries no actor; the event's author must be None.
        let nodes = history_from_json(serde_json::json!([
            {
                "createdAt": "2024-01-15T10:00:00Z",
                "actor": null,
                "fromState": { "name": "Backlog" },
                "toState": { "name": "Todo" }
            }
        ]));

        let events = linear_history_to_events(nodes);
        assert_eq!(events.len(), 1);
        assert!(events[0].author.is_none());
        assert_eq!(events[0].field, "status");
    }

    #[test]
    fn linear_history_title_only_node() {
        // A node with only a title transition produces exactly one title event,
        // and no spurious state/assignee/priority events.
        let nodes = history_from_json(serde_json::json!([
            {
                "createdAt": "2024-01-15T10:00:00Z",
                "actor": { "id": "u-carol", "name": "carol", "displayName": "Carol" },
                "fromTitle": "Old title",
                "toTitle": "New title"
            }
        ]));

        let events = linear_history_to_events(nodes);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].field, "title");
        assert_eq!(events[0].from.as_deref(), Some("Old title"));
        assert_eq!(events[0].to.as_deref(), Some("New title"));
    }

    #[test]
    fn linear_history_sorts_newest_first() {
        // Two nodes arrive oldest-first; output must be newest-first.
        let nodes = history_from_json(serde_json::json!([
            {
                "createdAt": "2024-01-10T09:00:00Z",
                "fromState": { "name": "Todo" },
                "toState": { "name": "In Progress" }
            },
            {
                "createdAt": "2024-01-15T14:30:00Z",
                "fromState": { "name": "In Progress" },
                "toState": { "name": "Done" }
            }
        ]));

        let events = linear_history_to_events(nodes);
        assert_eq!(events.len(), 2);
        // Newest (Done) first.
        assert_eq!(events[0].to.as_deref(), Some("Done"));
        assert_eq!(events[1].to.as_deref(), Some("In Progress"));
    }

    #[test]
    fn linear_history_skips_missing_timestamp_and_unchanged_fields() {
        let nodes = history_from_json(serde_json::json!([
            {
                // No createdAt -> skipped rather than fabricated.
                "toState": { "name": "Done" }
            },
            {
                "createdAt": "2024-02-01T00:00:00Z",
                // Priority present but unchanged -> no priority event.
                "fromPriority": 2,
                "toPriority": 2,
                // Assignee present but unchanged -> no assignee event.
                "fromAssignee": { "id": "u-dan", "name": "dan", "displayName": "Dan" },
                "toAssignee": { "id": "u-dan", "name": "dan", "displayName": "Dan" },
                // State present but unchanged -> no status event.
                "fromState": { "name": "In Progress" },
                "toState": { "name": "In Progress" },
                // Title present but unchanged -> no title event.
                "fromTitle": "Fix login",
                "toTitle": "Fix login"
            }
        ]));

        let events = linear_history_to_events(nodes);
        // The timestamp-less node is dropped, and the second node has nothing
        // that actually changed (priority, assignee, state, and title all equal).
        assert!(events.is_empty());
    }

    #[test]
    fn linear_history_emits_cleared_priority_and_title() {
        let nodes = history_from_json(serde_json::json!([
            {
                "createdAt": "2024-01-15T10:00:00Z",
                "fromPriority": 2,
                "toPriority": null,
                "fromTitle": "Old title",
                "toTitle": null
            }
        ]));

        let events = linear_history_to_events(nodes);
        assert_eq!(events.len(), 2);

        assert_eq!(events[0].field, "priority");
        assert_eq!(events[0].from.as_deref(), Some("High"));
        assert_eq!(events[0].to, None);

        assert_eq!(events[1].field, "title");
        assert_eq!(events[1].from.as_deref(), Some("Old title"));
        assert_eq!(events[1].to, None);
    }

    #[test]
    fn parses_linear_query_tokens() {
        let parsed = parse_linear_query("project: ORE state: {In Progress} #Bug login bug");
        assert_eq!(parsed.team.as_deref(), Some("ORE"));
        assert_eq!(parsed.state.as_deref(), Some("In Progress"));
        assert_eq!(parsed.labels, vec!["bug".to_string()]);
        assert_eq!(parsed.text, vec!["login".to_string(), "bug".to_string()]);
    }

    #[test]
    fn parses_unresolved_filter() {
        let parsed = parse_linear_query("team:ORE #Unresolved");
        assert_eq!(
            parsed.state_type_nin,
            Some(vec!["completed".to_string(), "canceled".to_string()])
        );
    }

    fn issue_from_json(value: serde_json::Value) -> LinearIssue {
        serde_json::from_value(value).unwrap()
    }

    fn text_value<'a>(fields: &'a [CustomField], name: &str) -> Option<&'a str> {
        fields.iter().find_map(|field| match field {
            CustomField::Text { name: n, value } if n == name => value.as_deref(),
            _ => None,
        })
    }

    fn single_enum_value<'a>(fields: &'a [CustomField], name: &str) -> Option<&'a str> {
        fields.iter().find_map(|field| match field {
            CustomField::SingleEnum { name: n, value } if n == name => value.as_deref(),
            _ => None,
        })
    }

    fn single_user<'a>(
        fields: &'a [CustomField],
        name: &str,
    ) -> Option<(Option<&'a str>, Option<&'a str>)> {
        fields.iter().find_map(|field| match field {
            CustomField::SingleUser {
                name: n,
                login,
                display_name,
            } if n == name => Some((login.as_deref(), display_name.as_deref())),
            _ => None,
        })
    }

    fn sample_issue_json() -> serde_json::Value {
        serde_json::json!({
            "id": "issue-1",
            "identifier": "ENG-1",
            "title": "Widen field projection",
            "description": "body",
            "priority": 2,
            "priorityLabel": "High",
            "estimate": 3.0,
            "dueDate": "2026-07-01",
            "url": "https://linear.app/x/issue/ENG-1",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-02T00:00:00Z",
            "team": { "id": "team-1", "key": "ENG", "name": "Engineering", "description": null },
            "state": { "id": "s-1", "name": "In Progress", "type": "started", "position": 1.0 },
            "assignee": { "id": "u-1", "name": "alice", "displayName": "Alice", "email": "alice@example.com" },
            "creator": { "id": "u-2", "name": "bob", "displayName": "Bob", "email": "bob@example.com" },
            "cycle": { "id": "c-1", "number": 7.0, "name": "Sprint 7" },
            "project": { "id": "p-1", "name": "Platform", "slugId": "plat", "description": null },
            "parent": null,
            "labels": { "nodes": [], "pageInfo": { "hasNextPage": false, "endCursor": null } }
        })
    }

    #[test]
    fn linear_issue_projects_widened_fields() {
        let issue = issue_from_json(sample_issue_json());
        let core = linear_issue_to_core(issue);
        let fields = &core.custom_fields;

        // Estimate is a Text field with the trailing `.0` stripped.
        assert_eq!(text_value(fields, "Estimate"), Some("3"));
        // Due Date passes through verbatim.
        assert_eq!(text_value(fields, "Due Date"), Some("2026-07-01"));
        // Cycle prefers the explicit name.
        assert_eq!(single_enum_value(fields, "Cycle"), Some("Sprint 7"));
        // Creator reuses the user helpers (login = email, display = displayName).
        assert_eq!(
            single_user(fields, "Creator"),
            Some((Some("bob@example.com"), Some("Bob")))
        );
    }

    #[test]
    fn linear_issue_omits_absent_widened_fields() {
        let mut json = sample_issue_json();
        let obj = json.as_object_mut().unwrap();
        obj.remove("estimate");
        obj.remove("dueDate");
        obj.remove("cycle");
        obj.remove("creator");

        let core = linear_issue_to_core(issue_from_json(json));
        let fields = &core.custom_fields;

        assert_eq!(text_value(fields, "Estimate"), None);
        assert_eq!(text_value(fields, "Due Date"), None);
        assert_eq!(single_enum_value(fields, "Cycle"), None);
        assert!(single_user(fields, "Creator").is_none());
    }

    #[test]
    fn linear_cycle_without_name_falls_back_to_number() {
        let mut json = sample_issue_json();
        json["cycle"] = serde_json::json!({ "id": "c-2", "number": 9.0, "name": null });

        let core = linear_issue_to_core(issue_from_json(json));
        assert_eq!(
            single_enum_value(&core.custom_fields, "Cycle"),
            Some("Cycle 9")
        );
    }

    #[test]
    fn linear_estimate_keeps_fractional_part() {
        let mut json = sample_issue_json();
        json["estimate"] = serde_json::json!(1.5);

        let core = linear_issue_to_core(issue_from_json(json));
        assert_eq!(text_value(&core.custom_fields, "Estimate"), Some("1.5"));
    }
}
