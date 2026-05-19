//! Model conversions from Linear types to tracker-core types.

use serde_json::{Value, json};
use tracker_core::{
    Comment, CommentAuthor, CustomField, Issue, IssueLink, IssueLinkType, IssueTag, LinkedIssue,
    Project, ProjectCustomField, ProjectRef, StateValueInfo, Tag, TagColor, User,
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
}
