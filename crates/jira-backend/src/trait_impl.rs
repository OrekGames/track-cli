//! Implementation of tracker-core traits for JiraClient

use tracker_core::{
    Comment, CreateIssue, CreateProject, CreateTag, Issue, IssueLink, IssueLinkType, IssueTag,
    IssueTracker, Project, ProjectCustomField, Result, TrackerError, UpdateIssue, User,
};

use crate::client::JiraClient;
use crate::convert::{
    create_issue_to_jira, get_standard_custom_fields, map_link_type, update_issue_to_jira,
};
use crate::models::{CreateJiraIssueLink, IssueKeyRef, IssueLinkTypeName};

impl IssueTracker for JiraClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        self.get_issue(id)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Issue>> {
        // If query looks like JQL, use it directly; otherwise, try simple conversion
        let jql = if query.contains('=') || query.contains(" AND ") || query.contains(" OR ") {
            query.to_string()
        } else {
            // Try to convert simple tracker-core query format to JQL
            convert_simple_query_to_jql(query)
        };

        self.search_issues(&jql, limit, skip)
            .map(|r| r.issues.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        let jira_issue = create_issue_to_jira(issue);
        self.create_issue(&jira_issue)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        let jira_update = update_issue_to_jira(update);
        self.update_issue(id, &jira_update)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn delete_issue(&self, id: &str) -> Result<()> {
        self.delete_issue(id).map_err(TrackerError::from)
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        self.list_projects()
            .map(|ps| ps.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        self.get_project(id)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn create_project(&self, _project: &CreateProject) -> Result<Project> {
        // Creating projects via API requires admin permissions and additional fields
        Err(TrackerError::InvalidInput(
            "Creating projects via API is not supported. Please use the Jira web interface."
                .to_string(),
        ))
    }

    fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        // In Jira, we use the project key (e.g., "SMS") as the identifier
        // because most Jira APIs expect the key, not the numeric ID
        self.get_project(identifier)
            .map(|p| p.key)
            .map_err(|_| TrackerError::ProjectNotFound(identifier.to_string()))
    }

    fn get_project_custom_fields(&self, _project_id: &str) -> Result<Vec<ProjectCustomField>> {
        // Return standard Jira fields
        // Note: Getting actual custom fields would require additional API calls
        Ok(get_standard_custom_fields())
    }

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        self.list_labels()
            .map(|labels| {
                labels
                    .into_iter()
                    .map(|name| IssueTag {
                        id: name.clone(),
                        name,
                        color: None,
                        issues_count: None,
                    })
                    .collect()
            })
            .map_err(TrackerError::from)
    }

    fn create_tag(&self, _tag: &CreateTag) -> Result<IssueTag> {
        Err(TrackerError::InvalidInput(
            "Jira labels cannot be created directly. They are created automatically when assigned to an issue. Use 'track issue update <ID> -t <label>' to create a label by assigning it.".to_string(),
        ))
    }

    fn delete_tag(&self, _name: &str) -> Result<()> {
        Err(TrackerError::InvalidInput(
            "Jira labels cannot be deleted via the REST API. Remove the label from all issues to effectively delete it, or use the Jira web interface.".to_string(),
        ))
    }

    fn update_tag(&self, _current_name: &str, _tag: &CreateTag) -> Result<IssueTag> {
        Err(TrackerError::InvalidInput(
            "Jira labels cannot be renamed via the REST API. Create a new label by assigning it to issues, then remove the old one.".to_string(),
        ))
    }

    fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        self.list_link_types()
            .map(|link_types| link_types.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn list_project_users(&self, project_id: &str) -> Result<Vec<User>> {
        self.list_assignable_users(project_id)
            .map(|users| users.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        // Get the issue to retrieve its links
        let issue = self.get_issue(issue_id).map_err(TrackerError::from)?;
        Ok(issue
            .fields
            .issuelinks
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn link_issues(
        &self,
        source: &str,
        target: &str,
        link_type: &str,
        _direction: &str,
    ) -> Result<()> {
        let jira_link_type = map_link_type(link_type);

        let link = CreateJiraIssueLink {
            link_type: IssueLinkTypeName {
                name: jira_link_type.to_string(),
            },
            inward_issue: IssueKeyRef {
                key: target.to_string(),
            },
            outward_issue: IssueKeyRef {
                key: source.to_string(),
            },
        };

        self.create_link(&link).map_err(TrackerError::from)
    }

    fn link_subtask(&self, _child: &str, _parent: &str) -> Result<()> {
        // In Jira, subtask relationship is set during issue creation
        // To convert an existing issue to a subtask, you need to move it
        // This is not directly supported via simple API call
        Err(TrackerError::InvalidInput(
            "Converting existing issues to subtasks is not supported. Create the issue as a subtask instead."
                .to_string(),
        ))
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        self.add_comment(issue_id, text)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        self.get_comments(issue_id)
            .map(|cs| cs.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }
}

/// Convert simple tracker-core query format to JQL
fn convert_simple_query_to_jql(query: &str) -> String {
    let mut parts = Vec::new();
    let mut remaining = query.trim();

    // Handle project: syntax
    if let Some(rest) = remaining.strip_prefix("project:") {
        let rest = rest.trim_start();
        if let Some(space_pos) = rest.find(' ') {
            let project = rest[..space_pos].trim();
            parts.push(format!("project = {}", project));
            remaining = &rest[space_pos..];
        } else {
            parts.push(format!("project = {}", rest.trim()));
            remaining = "";
        }
    }

    // Handle #hashtag syntax (states)
    let tokens: Vec<&str> = remaining.split_whitespace().collect();
    for token in tokens {
        if let Some(state) = token.strip_prefix('#') {
            match state.to_lowercase().as_str() {
                "unresolved" => parts.push("resolution IS EMPTY".to_string()),
                "resolved" => parts.push("resolution IS NOT EMPTY".to_string()),
                "open" => parts.push("status = Open".to_string()),
                "closed" => parts.push("status = Closed".to_string()),
                "done" => parts.push("status = Done".to_string()),
                "inprogress" | "in-progress" => parts.push("status = \"In Progress\"".to_string()),
                _ => parts.push(format!("status = \"{}\"", state)),
            }
        }
    }

    if parts.is_empty() {
        // If no conversion happened, use the query as-is (might be valid JQL)
        query.to_string()
    } else {
        parts.join(" AND ")
    }
}
