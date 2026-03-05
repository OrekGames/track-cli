//! Implementation of tracker-core traits for JiraClient

use tracker_core::{
    Comment, CreateIssue, CreateProject, CreateTag, Issue, IssueLink, IssueLinkType, IssueTag,
    IssueTracker, Project, ProjectCustomField, Result, SearchResult, TrackerError, UpdateIssue,
    User,
};

use crate::client::JiraClient;
use crate::convert::{
    create_issue_to_jira, get_standard_custom_fields, jira_field_to_project_custom_field,
    merge_fields, update_issue_to_jira,
};
use crate::models::{
    CreateJiraIssueLink, IssueKeyRef, IssueLinkTypeName, ParentId, UpdateJiraIssue,
    UpdateJiraIssueFields,
};

impl IssueTracker for JiraClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        Ok(self.get_issue(id)?.into())
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<SearchResult<Issue>> {
        // If query looks like JQL, use it directly; otherwise, try simple conversion
        let jql = if query.contains('=') || query.contains(" AND ") || query.contains(" OR ") {
            query.to_string()
        } else {
            // Try to convert simple tracker-core query format to JQL
            convert_simple_query_to_jql(query)
        };

        let r = self.search_issues(&jql, limit, skip)?;
        let total = r.total as u64;
        let items = r.issues.into_iter().map(Into::into).collect();
        Ok(SearchResult::with_total(items, total))
    }

    fn get_issue_count(&self, query: &str) -> Result<Option<u64>> {
        let jql = if query.contains('=') || query.contains(" AND ") || query.contains(" OR ") {
            query.to_string()
        } else {
            convert_simple_query_to_jql(query)
        };
        Ok(Some(self.count_issues(&jql)? as u64))
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        let fields = self.get_fields_cached();
        let jira_issue = create_issue_to_jira(issue, &fields);
        Ok(self.create_issue(&jira_issue)?.into())
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        let fields = self.get_fields_cached();
        let jira_update = update_issue_to_jira(update, &fields);
        Ok(self.update_issue(id, &jira_update)?.into())
    }

    fn delete_issue(&self, id: &str) -> Result<()> {
        Ok(self.delete_issue(id)?)
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        Ok(self.list_projects()?.into_iter().map(Into::into).collect())
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        Ok(self.get_project(id)?.into())
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
        let standard = get_standard_custom_fields();
        let instance_fields: Vec<ProjectCustomField> = self
            .get_fields_cached()
            .iter()
            .filter(|f| f.custom)
            .map(jira_field_to_project_custom_field)
            .collect();
        Ok(merge_fields(standard, instance_fields))
    }

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        Ok(self
            .list_labels()?
            .into_iter()
            .map(|name| IssueTag {
                id: name.clone(),
                name,
                color: None,
                issues_count: None,
            })
            .collect())
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
        Ok(self
            .list_link_types()?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn list_project_users(&self, project_id: &str) -> Result<Vec<User>> {
        Ok(self
            .list_assignable_users(project_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        // Get the issue to retrieve its links
        let issue = self.get_issue(issue_id)?;
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
        direction: &str,
    ) -> Result<()> {
        let jira_link_type = self.resolve_link_type(link_type);

        // Direction controls issue placement:
        // OUTWARD (depends): source depends on target → target blocks source
        //   → outward=target, inward=source
        // INWARD (required): source is required for target → source blocks target
        //   → outward=source, inward=target
        // BOTH (relates): bidirectional, order doesn't matter
        let (outward, inward) = match direction.to_uppercase().as_str() {
            "OUTWARD" => (target, source),
            _ => (source, target),
        };

        let link = CreateJiraIssueLink {
            link_type: IssueLinkTypeName {
                name: jira_link_type,
            },
            inward_issue: IssueKeyRef {
                key: inward.to_string(),
            },
            outward_issue: IssueKeyRef {
                key: outward.to_string(),
            },
        };

        Ok(self.create_link(&link)?)
    }

    fn unlink_issues(&self, _source: &str, link_id: &str) -> Result<()> {
        Ok(self.delete_link(link_id)?)
    }

    fn link_subtask(&self, child: &str, parent: &str) -> Result<()> {
        // Jira handles parent-child via the parent field — update the child issue
        let update = UpdateJiraIssue {
            fields: UpdateJiraIssueFields {
                parent: Some(ParentId {
                    key: Some(parent.to_string()),
                    id: None,
                }),
                ..Default::default()
            },
        };
        self.update_issue(child, &update)?;
        Ok(())
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        Ok(self.add_comment(issue_id, text)?.into())
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        Ok(self
            .get_comments(issue_id)?
            .into_iter()
            .map(Into::into)
            .collect())
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
