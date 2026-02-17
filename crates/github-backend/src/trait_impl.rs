//! Implementation of tracker-core traits for GitHubClient

use tracker_core::{
    Comment, CreateIssue, CreateProject, CreateTag, Issue, IssueLink, IssueTag, IssueTracker,
    Project, ProjectCustomField, Result, TrackerError, UpdateIssue,
};

use crate::client::GitHubClient;
use crate::convert::{
    convert_query_to_github, create_issue_from_core, get_standard_custom_fields,
    github_issue_to_core, update_issue_from_core,
};

/// Parse an issue number from a string identifier.
///
/// Accepts:
/// - A raw number: "42"
/// - An owner/repo#number format: "owner/repo#42"
fn parse_issue_number(id: &str) -> std::result::Result<u64, TrackerError> {
    // Try parsing as a raw number first
    if let Ok(n) = id.parse::<u64>() {
        return Ok(n);
    }

    // Try owner/repo#number format
    if let Some(hash_pos) = id.rfind('#') {
        let number_str = &id[hash_pos + 1..];
        if let Ok(n) = number_str.parse::<u64>() {
            return Ok(n);
        }
    }

    Err(TrackerError::InvalidInput(format!(
        "Invalid GitHub issue identifier: '{}'. Expected a number or 'owner/repo#number' format.",
        id
    )))
}

impl IssueTracker for GitHubClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        let number = parse_issue_number(id)?;
        let issue = self.get_issue(number).map_err(TrackerError::from)?;

        // If this is a PR, report as not found
        if issue.is_pull_request() {
            return Err(TrackerError::IssueNotFound(id.to_string()));
        }

        Ok(github_issue_to_core(issue, self.owner(), self.repo()))
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<Vec<Issue>> {
        let github_query = convert_query_to_github(query);
        let per_page = limit.min(100);
        let page = if per_page > 0 {
            (skip / per_page) + 1
        } else {
            1
        };

        let result = self
            .search_issues(&github_query, per_page, page)
            .map_err(TrackerError::from)?;

        let owner = self.owner().to_string();
        let repo = self.repo().to_string();

        Ok(result
            .items
            .into_iter()
            .filter(|i| !i.is_pull_request())
            .map(|i| github_issue_to_core(i, &owner, &repo))
            .collect())
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        let github_issue = create_issue_from_core(issue);
        let created = self
            .create_issue(&github_issue)
            .map_err(TrackerError::from)?;
        Ok(github_issue_to_core(created, self.owner(), self.repo()))
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        let number = parse_issue_number(id)?;
        let github_update = update_issue_from_core(update);
        let updated = self
            .update_issue(number, &github_update)
            .map_err(TrackerError::from)?;
        Ok(github_issue_to_core(updated, self.owner(), self.repo()))
    }

    fn delete_issue(&self, _id: &str) -> Result<()> {
        Err(TrackerError::InvalidInput(
            "GitHub does not support deleting issues. Use update to close them instead."
                .to_string(),
        ))
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        self.list_repos()
            .map(|repos| repos.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        // Parse "owner/repo" format
        let (owner, repo) = if let Some(slash_pos) = id.find('/') {
            (&id[..slash_pos], &id[slash_pos + 1..])
        } else {
            // If no slash, assume same owner, and id is the repo name
            (self.owner(), id)
        };

        self.get_repo(owner, repo)
            .map(Into::into)
            .map_err(|e| match e {
                crate::error::GitHubError::Api { status: 404, .. } => {
                    TrackerError::ProjectNotFound(id.to_string())
                }
                other => TrackerError::from(other),
            })
    }

    fn create_project(&self, _project: &CreateProject) -> Result<Project> {
        Err(TrackerError::InvalidInput(
            "Creating repositories via this tool is not supported. Please use the GitHub web interface or gh CLI."
                .to_string(),
        ))
    }

    fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        // For GitHub, the project identifier is "owner/repo"
        // If already in that format, validate it; otherwise build it
        if identifier.contains('/') {
            let project = self.get_project(identifier)?;
            Ok(project.short_name)
        } else {
            // Assume it's a repo name under the configured owner
            let full = format!("{}/{}", self.owner(), identifier);
            let project = self.get_project(&full)?;
            Ok(project.short_name)
        }
    }

    fn get_project_custom_fields(&self, _project_id: &str) -> Result<Vec<ProjectCustomField>> {
        Ok(get_standard_custom_fields())
    }

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        self.list_labels()
            .map(|labels| labels.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }

    fn create_tag(&self, tag: &CreateTag) -> Result<IssueTag> {
        let color = tag
            .color
            .as_deref()
            .unwrap_or("#ededed")
            .trim_start_matches('#')
            .to_string();

        let create = crate::models::CreateGitHubLabel {
            name: tag.name.clone(),
            color,
            description: tag.description.clone(),
        };

        self.create_label(&create)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn delete_tag(&self, name: &str) -> Result<()> {
        self.delete_label(name).map_err(TrackerError::from)
    }

    fn update_tag(&self, current_name: &str, tag: &CreateTag) -> Result<IssueTag> {
        let new_name = if tag.name != current_name {
            Some(tag.name.clone())
        } else {
            None
        };

        let color = tag
            .color
            .as_ref()
            .map(|c| c.trim_start_matches('#').to_string());

        let update = crate::models::UpdateGitHubLabel {
            new_name,
            color,
            description: tag.description.clone(),
        };

        self.update_label(current_name, &update)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn get_issue_links(&self, _issue_id: &str) -> Result<Vec<IssueLink>> {
        // GitHub has no formal issue link system
        Ok(Vec::new())
    }

    fn link_issues(
        &self,
        _source: &str,
        _target: &str,
        _link_type: &str,
        _direction: &str,
    ) -> Result<()> {
        Err(TrackerError::InvalidInput(
            "GitHub does not support formal issue links. Reference issues via #number in comments instead."
                .to_string(),
        ))
    }

    fn link_subtask(&self, _child: &str, _parent: &str) -> Result<()> {
        Err(TrackerError::InvalidInput(
            "GitHub does not support subtask relationships. Use task lists in issue body or reference via #number."
                .to_string(),
        ))
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        let number = parse_issue_number(issue_id)?;
        self.add_comment(number, text)
            .map(Into::into)
            .map_err(TrackerError::from)
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        let number = parse_issue_number(issue_id)?;
        self.get_comments(number)
            .map(|cs| cs.into_iter().map(Into::into).collect())
            .map_err(TrackerError::from)
    }
}
