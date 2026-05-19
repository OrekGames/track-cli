use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::time::Duration;
use ureq::Agent;

use crate::error::{LinearError, Result};
use crate::models::*;

const DEFAULT_LINEAR_API_URL: &str = "https://api.linear.app/graphql";

const ISSUE_FIELDS: &str = r#"
    id
    identifier
    title
    description
    priority
    priorityLabel
    url
    createdAt
    updatedAt
    team { id key name description }
    state { id name type position }
    assignee { id name displayName email }
    project { id name slugId description }
    parent { id identifier title }
    labels(first: 100) {
      nodes { id name color description }
      pageInfo { hasNextPage endCursor }
    }
"#;

const ISSUE_REF_FIELDS: &str = r#"
    id
    identifier
    title
"#;

const TEAM_FIELDS: &str = r#"
    id
    key
    name
    description
"#;

/// Linear GraphQL API client.
pub struct LinearClient {
    agent: Agent,
    api_url: String,
    token: String,
    default_team: Option<String>,
    default_linear_project: Option<String>,
    link_mappings: HashMap<String, String>,
}

impl LinearClient {
    /// Create a new Linear client targeting the public Linear GraphQL API.
    pub fn new(token: &str) -> Self {
        Self::with_base_url(DEFAULT_LINEAR_API_URL, token)
    }

    /// Create a new Linear client with a custom GraphQL endpoint.
    pub fn with_base_url(api_url: &str, token: &str) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();

        Self {
            agent,
            api_url: api_url.to_string(),
            token: token.to_string(),
            default_team: None,
            default_linear_project: None,
            link_mappings: HashMap::new(),
        }
    }

    /// Set Linear-specific defaults from configuration.
    pub fn with_defaults(
        mut self,
        default_team: Option<String>,
        default_linear_project: Option<String>,
    ) -> Self {
        self.default_team = default_team;
        self.default_linear_project = default_linear_project;
        self
    }

    /// Set custom relation type mappings (canonical name -> Linear relation type).
    pub fn with_link_mappings(mut self, mappings: HashMap<String, String>) -> Self {
        self.link_mappings = mappings;
        self
    }

    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    pub fn default_team(&self) -> Option<&str> {
        self.default_team.as_deref()
    }

    pub fn default_linear_project(&self) -> Option<&str> {
        self.default_linear_project.as_deref()
    }

    pub(crate) fn resolve_link_type(&self, canonical: &str) -> String {
        if let Some(mapped) = self.link_mappings.get(canonical) {
            return mapped.clone();
        }

        match canonical {
            "relates" | "related" => "related",
            "duplicates" | "duplicate" | "duplicated-by" => "duplicate",
            "similar" => "similar",
            "blocks" | "depends" | "required" => "blocks",
            _ => canonical,
        }
        .to_string()
    }

    pub(crate) fn graphql<T, V>(&self, query: &str, variables: V) -> Result<T>
    where
        T: DeserializeOwned,
        V: serde::Serialize,
    {
        let request = GraphQlRequest { query, variables };
        let response = self
            .agent
            .post(&self.api_url)
            .header("Authorization", &self.token)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send_json(&request)
            .map_err(LinearError::Http)?;

        self.parse_graphql_response(response)
    }

    fn parse_graphql_response<T>(&self, mut response: ureq::http::Response<ureq::Body>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| String::new());

        if !(200..300).contains(&status) {
            if status == 401 {
                return Err(LinearError::Unauthorized);
            }
            if status == 429 {
                return Err(LinearError::RateLimited);
            }

            let message = parse_error_message(&body).unwrap_or_else(|| {
                if body.is_empty() {
                    format!("HTTP {}", status)
                } else {
                    body
                }
            });

            return Err(LinearError::Api { status, message });
        }

        let envelope: GraphQlResponse<T> = serde_json::from_str(&body)?;
        if let Some(errors) = envelope.errors
            && !errors.is_empty()
        {
            if errors.iter().any(|error| {
                error
                    .extensions
                    .as_ref()
                    .and_then(|e| e.code.as_deref())
                    .is_some_and(|code| code.eq_ignore_ascii_case("RATELIMITED"))
            }) {
                return Err(LinearError::RateLimited);
            }

            let message = errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(LinearError::Api {
                status: 200,
                message,
            });
        }

        envelope.data.ok_or_else(|| LinearError::Api {
            status: 200,
            message: "Linear GraphQL response did not contain data".to_string(),
        })
    }

    pub fn get_issue(&self, id: &str) -> Result<LinearIssue> {
        #[derive(serde::Deserialize)]
        struct Data {
            issue: Option<LinearIssue>,
        }

        let query = format!(
            r#"
            query Issue($id: String!) {{
              issue(id: $id) {{
                {ISSUE_FIELDS}
              }}
            }}
            "#
        );

        let data: Data = self.graphql(&query, json!({ "id": id }))?;
        data.issue
            .ok_or_else(|| LinearError::IssueNotFound(id.to_string()))
    }

    pub fn list_teams(&self) -> Result<Vec<LinearTeam>> {
        #[derive(serde::Deserialize)]
        struct Data {
            teams: LinearConnection<LinearTeam>,
        }

        let query = format!(
            r#"
            query Teams($first: Int!, $after: String) {{
              teams(first: $first, after: $after) {{
                nodes {{ {TEAM_FIELDS} }}
                pageInfo {{ hasNextPage endCursor }}
              }}
            }}
            "#
        );

        let mut teams = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let data: Data = self.graphql(
                &query,
                json!({
                    "first": 100,
                    "after": after,
                }),
            )?;
            teams.extend(data.teams.nodes);
            if !data.teams.page_info.has_next_page {
                break;
            }
            after = data.teams.page_info.end_cursor;
        }
        Ok(teams)
    }

    pub fn get_team(&self, id: &str) -> Result<LinearTeam> {
        #[derive(serde::Deserialize)]
        struct Data {
            team: Option<LinearTeam>,
        }

        let query = format!(
            r#"
            query Team($id: String!) {{
              team(id: $id) {{ {TEAM_FIELDS} }}
            }}
            "#
        );

        match self.graphql::<Data, _>(&query, json!({ "id": id })) {
            Ok(data) => {
                if let Some(team) = data.team {
                    return Ok(team);
                }
            }
            Err(LinearError::Api { .. }) | Err(LinearError::ProjectNotFound(_)) => {}
            Err(other) => return Err(other),
        }

        self.list_teams()?
            .into_iter()
            .find(|team| team_matches(team, id))
            .ok_or_else(|| LinearError::ProjectNotFound(id.to_string()))
    }

    pub fn resolve_team_id(&self, identifier: &str) -> Result<String> {
        Ok(self.get_team(identifier)?.id)
    }

    pub fn get_team_details(&self, id: &str) -> Result<LinearTeamDetails> {
        #[derive(Debug, serde::Deserialize)]
        struct Data {
            team: Option<LinearTeamDetails>,
        }

        let query = format!(
            r#"
            query TeamDetails($id: String!) {{
              team(id: $id) {{
                {TEAM_FIELDS}
                states(first: 100) {{
                  nodes {{ id name type position }}
                  pageInfo {{ hasNextPage endCursor }}
                }}
                members(first: 100) {{
                  nodes {{ id name displayName email }}
                  pageInfo {{ hasNextPage endCursor }}
                }}
                labels(first: 100) {{
                  nodes {{ id name color description }}
                  pageInfo {{ hasNextPage endCursor }}
                }}
                projects(first: 100) {{
                  nodes {{ id name slugId description }}
                  pageInfo {{ hasNextPage endCursor }}
                }}
              }}
            }}
            "#
        );

        let data: Data = self.graphql(&query, json!({ "id": id }))?;
        data.team
            .ok_or_else(|| LinearError::ProjectNotFound(id.to_string()))
    }

    pub fn list_team_states(&self, team_id: &str) -> Result<Vec<LinearWorkflowState>> {
        Ok(self.get_team_details(team_id)?.states.nodes)
    }

    pub fn list_team_members(&self, team_id: &str) -> Result<Vec<LinearUser>> {
        Ok(self.get_team_details(team_id)?.members.nodes)
    }

    pub fn list_team_labels(&self, team_id: &str) -> Result<Vec<LinearIssueLabel>> {
        Ok(self.get_team_details(team_id)?.labels.nodes)
    }

    pub fn list_team_projects(&self, team_id: &str) -> Result<Vec<LinearProject>> {
        Ok(self.get_team_details(team_id)?.projects.nodes)
    }

    pub fn list_issue_labels(&self, team_id: Option<&str>) -> Result<Vec<LinearIssueLabel>> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueLabels")]
            issue_labels: LinearConnection<LinearIssueLabel>,
        }

        let query = r#"
            query IssueLabels($first: Int!, $after: String, $filter: IssueLabelFilter) {
              issueLabels(first: $first, after: $after, filter: $filter) {
                nodes { id name color description }
                pageInfo { hasNextPage endCursor }
              }
            }
        "#;

        let filter = team_id.map(|id| {
            json!({
                "team": {
                    "id": { "eq": id }
                }
            })
        });

        let mut labels = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let data: Data = self.graphql(
                query,
                json!({
                    "first": 100,
                    "after": after,
                    "filter": filter,
                }),
            )?;
            labels.extend(data.issue_labels.nodes);
            if !data.issue_labels.page_info.has_next_page {
                break;
            }
            after = data.issue_labels.page_info.end_cursor;
        }
        Ok(labels)
    }

    pub fn find_label(&self, team_id: &str, name_or_id: &str) -> Result<LinearIssueLabel> {
        self.list_team_labels(team_id)?
            .into_iter()
            .find(|label| id_or_name_matches(&label.id, &label.name, name_or_id))
            .ok_or_else(|| LinearError::Api {
                status: 400,
                message: format!("Linear label '{}' was not found in team", name_or_id),
            })
    }

    pub fn find_project(&self, team_id: &str, name_or_id_or_slug: &str) -> Result<LinearProject> {
        self.list_team_projects(team_id)?
            .into_iter()
            .find(|project| {
                project.id == name_or_id_or_slug
                    || project
                        .slug_id
                        .as_deref()
                        .is_some_and(|slug| slug.eq_ignore_ascii_case(name_or_id_or_slug))
                    || project.name.eq_ignore_ascii_case(name_or_id_or_slug)
            })
            .ok_or_else(|| LinearError::ProjectNotFound(name_or_id_or_slug.to_string()))
    }

    pub fn find_user(&self, team_id: &str, login_or_name: &str) -> Result<LinearUser> {
        if login_or_name.eq_ignore_ascii_case("me") {
            return self.viewer();
        }

        self.list_team_members(team_id)?
            .into_iter()
            .find(|user| user_matches(user, login_or_name))
            .ok_or_else(|| LinearError::Api {
                status: 400,
                message: format!("Linear user '{}' was not found in team", login_or_name),
            })
    }

    pub fn viewer(&self) -> Result<LinearUser> {
        #[derive(serde::Deserialize)]
        struct Data {
            viewer: LinearUser,
        }

        let data: Data = self.graphql(
            "query Viewer { viewer { id name displayName email } }",
            json!({}),
        )?;
        Ok(data.viewer)
    }

    pub fn search_issues_page(
        &self,
        filter: Option<Value>,
        term: Option<&str>,
        first: usize,
        after: Option<String>,
    ) -> Result<(Vec<LinearIssue>, Option<u64>, LinearPageInfo)> {
        #[derive(serde::Deserialize)]
        struct SearchData {
            #[serde(rename = "searchIssues")]
            search_issues: LinearConnection<LinearIssue>,
        }

        #[derive(serde::Deserialize)]
        struct IssuesData {
            issues: LinearConnection<LinearIssue>,
        }

        let first = first.clamp(1, 100);
        if let Some(term) = term
            && !term.trim().is_empty()
        {
            let query = format!(
                r#"
                query SearchIssues($term: String!, $first: Int!, $after: String, $filter: IssueFilter) {{
                  searchIssues(term: $term, first: $first, after: $after, filter: $filter) {{
                    totalCount
                    nodes {{ {ISSUE_FIELDS} }}
                    pageInfo {{ hasNextPage endCursor }}
                  }}
                }}
                "#
            );
            let data: SearchData = self.graphql(
                &query,
                json!({
                    "term": term,
                    "first": first,
                    "after": after,
                    "filter": filter,
                }),
            )?;
            let total = data.search_issues.total_count;
            return Ok((
                data.search_issues.nodes,
                total,
                data.search_issues.page_info,
            ));
        }

        let query = format!(
            r#"
            query Issues($first: Int!, $after: String, $filter: IssueFilter) {{
              issues(first: $first, after: $after, filter: $filter, orderBy: updatedAt) {{
                nodes {{ {ISSUE_FIELDS} }}
                pageInfo {{ hasNextPage endCursor }}
              }}
            }}
            "#
        );
        let data: IssuesData = self.graphql(
            &query,
            json!({
                "first": first,
                "after": after,
                "filter": filter,
            }),
        )?;
        Ok((data.issues.nodes, None, data.issues.page_info))
    }

    pub fn create_issue(&self, input: &LinearIssueCreateInput) -> Result<LinearIssue> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueCreate")]
            issue_create: LinearIssuePayload,
        }

        let query = format!(
            r#"
            mutation IssueCreate($input: IssueCreateInput!) {{
              issueCreate(input: $input) {{
                success
                issue {{ {ISSUE_FIELDS} }}
              }}
            }}
            "#
        );

        let data: Data = self.graphql(&query, json!({ "input": input }))?;
        issue_from_payload(data.issue_create, "issueCreate")
    }

    pub fn update_issue(&self, id: &str, input: &LinearIssueUpdateInput) -> Result<LinearIssue> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueUpdate")]
            issue_update: LinearIssuePayload,
        }

        let query = format!(
            r#"
            mutation IssueUpdate($id: String!, $input: IssueUpdateInput!) {{
              issueUpdate(id: $id, input: $input) {{
                success
                issue {{ {ISSUE_FIELDS} }}
              }}
            }}
            "#
        );

        let data: Data = self.graphql(&query, json!({ "id": id, "input": input }))?;
        issue_from_payload(data.issue_update, "issueUpdate")
    }

    pub fn delete_issue(&self, id: &str) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueDelete")]
            issue_delete: LinearDeletePayload,
        }

        let query = r#"
            mutation IssueDelete($id: String!, $permanentlyDelete: Boolean!) {
              issueDelete(id: $id, permanentlyDelete: $permanentlyDelete) {
                success
              }
            }
        "#;

        let data: Data = self.graphql(
            query,
            json!({
                "id": id,
                "permanentlyDelete": false,
            }),
        )?;

        if data.issue_delete.success {
            Ok(())
        } else {
            Err(LinearError::Api {
                status: 400,
                message: "Linear issueDelete returned success=false".to_string(),
            })
        }
    }

    pub fn add_comment(&self, issue_id: &str, body: &str) -> Result<LinearComment> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "commentCreate")]
            comment_create: LinearCommentPayload,
        }

        let query = r#"
            mutation CommentCreate($input: CommentCreateInput!) {
              commentCreate(input: $input) {
                success
                comment {
                  id
                  body
                  createdAt
                  user { id name displayName email }
                }
              }
            }
        "#;

        let input = LinearCommentCreateInput {
            issue_id: issue_id.to_string(),
            body: body.to_string(),
        };
        let data: Data = self.graphql(query, json!({ "input": input }))?;
        if data.comment_create.success {
            data.comment_create.comment.ok_or_else(|| LinearError::Api {
                status: 400,
                message: "Linear commentCreate returned no comment".to_string(),
            })
        } else {
            Err(LinearError::Api {
                status: 400,
                message: "Linear commentCreate returned success=false".to_string(),
            })
        }
    }

    pub fn get_comments_page(
        &self,
        issue_id: &str,
        first: usize,
        after: Option<String>,
    ) -> Result<(Vec<LinearComment>, LinearPageInfo)> {
        #[derive(serde::Deserialize)]
        struct IssueComments {
            comments: LinearConnection<LinearComment>,
        }

        #[derive(serde::Deserialize)]
        struct Data {
            issue: Option<IssueComments>,
        }

        let query = r#"
            query IssueComments($id: String!, $first: Int!, $after: String) {
              issue(id: $id) {
                comments(first: $first, after: $after) {
                  nodes {
                    id
                    body
                    createdAt
                    user { id name displayName email }
                  }
                  pageInfo { hasNextPage endCursor }
                }
              }
            }
        "#;

        let data: Data = self.graphql(
            query,
            json!({
                "id": issue_id,
                "first": first.clamp(1, 100),
                "after": after,
            }),
        )?;
        let issue = data
            .issue
            .ok_or_else(|| LinearError::IssueNotFound(issue_id.to_string()))?;
        Ok((issue.comments.nodes, issue.comments.page_info))
    }

    pub fn create_label(&self, input: &LinearIssueLabelCreateInput) -> Result<LinearIssueLabel> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueLabelCreate")]
            issue_label_create: LinearIssueLabelPayload,
        }

        let query = r#"
            mutation IssueLabelCreate($input: IssueLabelCreateInput!) {
              issueLabelCreate(input: $input) {
                success
                issueLabel { id name color description }
              }
            }
        "#;

        let data: Data = self.graphql(query, json!({ "input": input }))?;
        label_from_payload(data.issue_label_create, "issueLabelCreate")
    }

    pub fn update_label(
        &self,
        id: &str,
        input: &LinearIssueLabelUpdateInput,
    ) -> Result<LinearIssueLabel> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueLabelUpdate")]
            issue_label_update: LinearIssueLabelPayload,
        }

        let query = r#"
            mutation IssueLabelUpdate($id: String!, $input: IssueLabelUpdateInput!) {
              issueLabelUpdate(id: $id, input: $input) {
                success
                issueLabel { id name color description }
              }
            }
        "#;

        let data: Data = self.graphql(query, json!({ "id": id, "input": input }))?;
        label_from_payload(data.issue_label_update, "issueLabelUpdate")
    }

    pub fn delete_label(&self, id: &str) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueLabelDelete")]
            issue_label_delete: LinearDeletePayload,
        }

        let query = r#"
            mutation IssueLabelDelete($id: String!) {
              issueLabelDelete(id: $id) { success }
            }
        "#;

        let data: Data = self.graphql(query, json!({ "id": id }))?;
        if data.issue_label_delete.success {
            Ok(())
        } else {
            Err(LinearError::Api {
                status: 400,
                message: "Linear issueLabelDelete returned success=false".to_string(),
            })
        }
    }

    pub fn get_issue_relations(&self, issue_id: &str) -> Result<LinearIssueRelations> {
        #[derive(Debug, serde::Deserialize)]
        struct IssueRelationsData {
            parent: Option<LinearIssueRef>,
            children: LinearConnection<LinearIssueRef>,
            relations: LinearConnection<LinearIssueRelation>,
        }

        #[derive(Debug, serde::Deserialize)]
        struct Data {
            issue: Option<IssueRelationsData>,
        }

        let query = format!(
            r#"
            query IssueRelations($id: String!) {{
              issue(id: $id) {{
                parent {{ {ISSUE_REF_FIELDS} }}
                children(first: 100) {{
                  nodes {{ {ISSUE_REF_FIELDS} }}
                  pageInfo {{ hasNextPage endCursor }}
                }}
                relations(first: 100) {{
                  nodes {{
                    id
                    type
                    issue {{ {ISSUE_REF_FIELDS} }}
                    relatedIssue {{ {ISSUE_REF_FIELDS} }}
                  }}
                  pageInfo {{ hasNextPage endCursor }}
                }}
              }}
            }}
            "#
        );

        let data: Data = self.graphql(&query, json!({ "id": issue_id }))?;
        let issue = data
            .issue
            .ok_or_else(|| LinearError::IssueNotFound(issue_id.to_string()))?;

        Ok(LinearIssueRelations {
            parent: issue.parent,
            children: issue.children.nodes,
            relations: issue.relations.nodes,
        })
    }

    pub fn create_issue_relation(
        &self,
        input: &LinearIssueRelationCreateInput,
    ) -> Result<LinearIssueRelation> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueRelationCreate")]
            issue_relation_create: LinearIssueRelationPayload,
        }

        let query = format!(
            r#"
            mutation IssueRelationCreate($input: IssueRelationCreateInput!) {{
              issueRelationCreate(input: $input) {{
                success
                issueRelation {{
                  id
                  type
                  issue {{ {ISSUE_REF_FIELDS} }}
                  relatedIssue {{ {ISSUE_REF_FIELDS} }}
                }}
              }}
            }}
            "#
        );

        let data: Data = self.graphql(&query, json!({ "input": input }))?;
        if data.issue_relation_create.success {
            data.issue_relation_create
                .issue_relation
                .ok_or_else(|| LinearError::Api {
                    status: 400,
                    message: "Linear issueRelationCreate returned no relation".to_string(),
                })
        } else {
            Err(LinearError::Api {
                status: 400,
                message: "Linear issueRelationCreate returned success=false".to_string(),
            })
        }
    }

    pub fn delete_issue_relation(&self, id: &str) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct Data {
            #[serde(rename = "issueRelationDelete")]
            issue_relation_delete: LinearDeletePayload,
        }

        let query = r#"
            mutation IssueRelationDelete($id: String!) {
              issueRelationDelete(id: $id) { success }
            }
        "#;

        let data: Data = self.graphql(query, json!({ "id": id }))?;
        if data.issue_relation_delete.success {
            Ok(())
        } else {
            Err(LinearError::Api {
                status: 400,
                message: "Linear issueRelationDelete returned success=false".to_string(),
            })
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LinearTeamDetails {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub states: LinearConnection<LinearWorkflowState>,
    pub members: LinearConnection<LinearUser>,
    pub labels: LinearConnection<LinearIssueLabel>,
    pub projects: LinearConnection<LinearProject>,
}

#[derive(Debug, Clone)]
pub struct LinearIssueRelations {
    pub parent: Option<LinearIssueRef>,
    pub children: Vec<LinearIssueRef>,
    pub relations: Vec<LinearIssueRelation>,
}

pub(crate) fn filter_with_team(team_id: Option<&str>) -> Option<Value> {
    team_id.map(|id| {
        json!({
            "team": {
                "id": { "eq": id }
            }
        })
    })
}

pub(crate) fn add_filter_condition(filter: &mut Option<Value>, condition: Value) {
    match filter {
        None => *filter = Some(condition),
        Some(Value::Object(existing)) => {
            if let Value::Object(new_fields) = condition {
                merge_filter_objects(existing, new_fields);
            }
        }
        Some(_) => {}
    }
}

fn merge_filter_objects(existing: &mut Map<String, Value>, new_fields: Map<String, Value>) {
    for (key, value) in new_fields {
        existing.insert(key, value);
    }
}

fn issue_from_payload(payload: LinearIssuePayload, operation: &str) -> Result<LinearIssue> {
    if payload.success {
        payload.issue.ok_or_else(|| LinearError::Api {
            status: 400,
            message: format!("Linear {operation} returned no issue"),
        })
    } else {
        Err(LinearError::Api {
            status: 400,
            message: format!("Linear {operation} returned success=false"),
        })
    }
}

fn label_from_payload(
    payload: LinearIssueLabelPayload,
    operation: &str,
) -> Result<LinearIssueLabel> {
    if payload.success {
        payload.issue_label.ok_or_else(|| LinearError::Api {
            status: 400,
            message: format!("Linear {operation} returned no issue label"),
        })
    } else {
        Err(LinearError::Api {
            status: 400,
            message: format!("Linear {operation} returned success=false"),
        })
    }
}

fn parse_error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    if let Some(errors) = value.get("errors").and_then(|v| v.as_array()) {
        let messages = errors
            .iter()
            .filter_map(|error| error.get("message").and_then(|v| v.as_str()))
            .collect::<Vec<_>>();
        if !messages.is_empty() {
            return Some(messages.join("; "));
        }
    }
    value
        .get("message")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn team_matches(team: &LinearTeam, identifier: &str) -> bool {
    team.id == identifier
        || team.key.eq_ignore_ascii_case(identifier)
        || team.name.eq_ignore_ascii_case(identifier)
}

fn id_or_name_matches(id: &str, name: &str, identifier: &str) -> bool {
    id == identifier || name.eq_ignore_ascii_case(identifier)
}

fn user_matches(user: &LinearUser, identifier: &str) -> bool {
    user.id == identifier
        || user.name.eq_ignore_ascii_case(identifier)
        || user
            .display_name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(identifier))
        || user
            .email
            .as_deref()
            .is_some_and(|email| email.eq_ignore_ascii_case(identifier))
}
