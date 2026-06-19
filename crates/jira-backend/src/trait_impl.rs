//! Implementation of tracker-core traits for JiraClient

use tracker_core::{
    AttachmentUpload, Comment, CreateIssue, CreateProject, CreateTag, Issue, IssueAttachment,
    IssueHistoryEvent, IssueLink, IssueLinkType, IssueTag, IssueTracker, Project,
    ProjectCustomField, Result, SearchResult, TrackerError, UpdateIssue, User,
};

use crate::client::JiraClient;
use crate::convert::{
    create_issue_to_jira, get_standard_custom_fields, jira_changelog_to_history_events,
    jira_field_to_project_custom_field, jira_issue_to_core, merge_fields, parse_jira_datetime,
    update_issue_to_jira,
};
use crate::models::{
    CreateJiraIssueLink, IssueKeyRef, IssueLinkTypeName, ParentId, UpdateJiraIssue,
    UpdateJiraIssueFields,
};

impl IssueTracker for JiraClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        let issue = self.get_issue(id)?;
        let fields = self.get_fields_cached();
        Ok(jira_issue_to_core(issue, &fields))
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<SearchResult<Issue>> {
        if limit == 0 {
            return Ok(SearchResult::from_items(Vec::new()));
        }

        let jql = to_jql(query);
        let fields = self.get_fields_cached();

        // /search/jql is cursor-based (`startAt` is silently ignored), so an
        // offset window has to be emulated: walk the token chain, discard
        // `skip` issues, then collect up to `limit`.
        let mut items: Vec<Issue> = Vec::new();
        let mut remaining_skip = skip;
        let mut token: Option<String> = None;
        let mut no_progress_pages = 0usize;

        loop {
            // Ask for exactly what's still needed; the server caps page
            // sizes (~100 with fields=*all) and may return fewer.
            let page_size = remaining_skip.saturating_add(limit - items.len()).min(100);

            let r = self.search_issues(&jql, page_size, token.as_deref())?;
            let page_len = r.issues.len();

            if remaining_skip >= page_len {
                remaining_skip -= page_len;
            } else {
                let need = limit - items.len();
                items.extend(
                    r.issues
                        .into_iter()
                        .skip(remaining_skip)
                        .take(need)
                        .map(|i| jira_issue_to_core(i, &fields)),
                );
                remaining_skip = 0;
            }

            // A short (even empty) page is NOT a stop condition: the server
            // legitimately returns fewer than `maxResults` rows mid-stream.
            // Token absence is the authoritative last-page signal; `is_last`
            // is only trustworthy when true.
            if items.len() >= limit || r.is_last {
                break;
            }
            match r.next_page_token {
                None => break,
                Some(t) if token.as_deref() == Some(t.as_str()) => {
                    return Err(TrackerError::PaginationStalled(
                        "server returned the same nextPageToken twice".to_string(),
                    ));
                }
                Some(t) => {
                    // Non-empty pages always make progress (toward skip or
                    // items); only a run of empty pages can walk in circles.
                    if page_len == 0 {
                        no_progress_pages += 1;
                        if no_progress_pages >= MAX_NO_PROGRESS_PAGES {
                            return Err(TrackerError::PaginationStalled(
                                "server keeps returning empty pages with fresh page tokens"
                                    .to_string(),
                            ));
                        }
                    } else {
                        no_progress_pages = 0;
                    }
                    token = Some(t);
                }
            }
        }

        Ok(SearchResult::from_items(items))
    }

    fn get_issue_count(&self, _query: &str) -> Result<Option<u64>> {
        // The new /search/jql endpoint does not return a total count,
        // and there is no separate count endpoint on Jira Cloud.
        Ok(None)
    }

    fn search_all_issues(&self, query: &str, max_results: usize) -> Result<Vec<Issue>> {
        let jql = to_jql(query);
        let fields = self.get_fields_cached();

        // Native cursor walk: O(pages) requests, unlike the default
        // implementation which would re-walk the token chain from page 1
        // for every offset window.
        let mut all: Vec<Issue> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut token: Option<String> = None;
        let mut no_progress_pages = 0usize;

        loop {
            let remaining = max_results.saturating_sub(all.len());
            if remaining == 0 {
                break;
            }

            // /search/jql hard-caps maxResults at 100 for full-field requests
            let page_size = remaining.min(100);
            let r = self.search_issues(&jql, page_size, token.as_deref())?;

            let before = all.len();
            for ji in r.issues {
                let issue = jira_issue_to_core(ji, &fields);
                if seen.insert(issue.id.clone()) {
                    all.push(issue);
                }
            }

            // Termination first (a fully-duplicate or empty FINAL page is a
            // legitimate end, not a stall), then anomaly detection.
            if r.is_last {
                break;
            }
            match r.next_page_token {
                // Absent token = authoritative last page
                None => break,
                Some(t) if token.as_deref() == Some(t.as_str()) => {
                    return Err(TrackerError::PaginationStalled(
                        "server returned the same nextPageToken twice".to_string(),
                    ));
                }
                Some(t) => {
                    // Empty or fully-duplicate pages can occur mid-stream;
                    // keep following fresh tokens, but fail loudly if the
                    // cursor stops yielding new issues altogether rather
                    // than returning a silently incomplete result set.
                    if all.len() == before {
                        no_progress_pages += 1;
                        if no_progress_pages >= MAX_NO_PROGRESS_PAGES {
                            return Err(TrackerError::PaginationStalled(
                                "cursor pages stopped yielding new issues".to_string(),
                            ));
                        }
                    } else {
                        no_progress_pages = 0;
                    }
                    token = Some(t);
                }
            }
        }

        Ok(all)
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        let fields = self.get_fields_cached();
        let jira_issue = create_issue_to_jira(issue, &fields)?;
        let created = self.create_issue(&jira_issue)?;
        Ok(jira_issue_to_core(created, &fields))
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        use tracker_core::CustomFieldUpdate;

        let fields = self.get_fields_cached();

        // 1. Separate the state change, if present.
        let (status_update, other_fields): (Vec<_>, Vec<_>) = update
            .custom_fields
            .iter()
            .cloned()
            .partition(|cf| matches!(cf, CustomFieldUpdate::State { .. }));

        let mut status_targets: Vec<String> = status_update
            .into_iter()
            .filter_map(|cf| match cf {
                CustomFieldUpdate::State { value, .. } => Some(value),
                _ => None,
            })
            .collect();
        status_targets.sort();
        status_targets.dedup();
        if status_targets.len() > 1 {
            return Err(TrackerError::InvalidInput(format!(
                "Conflicting status values requested in one update ({}). \
                 Jira supports a single status transition per update.",
                status_targets.join(", ")
            )));
        }
        let status_target = status_targets.pop();

        let stripped = UpdateIssue {
            custom_fields: other_fields,
            ..update.clone()
        };

        // 2. PUT the remaining fields (skip the call entirely if nothing changed).
        let has_field_updates = stripped.summary.is_some()
            || stripped.description.is_some()
            || !stripped.custom_fields.is_empty()
            || !stripped.tags.is_empty()
            || stripped.parent.is_some();

        if has_field_updates {
            let jira_update = update_issue_to_jira(&stripped, &fields)?;
            self.update_issue(id, &jira_update)?;
        }

        // 3. POST the transition, if requested.
        if let Some(target) = status_target {
            let transition_id = self.resolve_transition_id(id, &target)?;
            self.transition_issue(id, &transition_id)?;
        }

        // 4. Re-fetch the fresh issue (matches current behavior).
        Ok(jira_issue_to_core(self.get_issue(id)?, &fields))
    }

    fn delete_issue(&self, id: &str) -> Result<()> {
        Ok(self.delete_issue(id)?)
    }

    fn list_issue_attachments(&self, issue_id: &str) -> Result<Vec<IssueAttachment>> {
        let issue = self.get_issue(issue_id)?;
        Ok(issue
            .fields
            .attachment
            .into_iter()
            .map(jira_attachment_to_core)
            .collect())
    }

    fn add_issue_attachment(
        &self,
        issue_id: &str,
        upload: &AttachmentUpload,
    ) -> Result<Vec<IssueAttachment>> {
        Ok(self
            .add_issue_attachments(issue_id, upload)?
            .into_iter()
            .map(jira_attachment_to_core)
            .collect())
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

    fn get_project_custom_fields(&self, project_id: &str) -> Result<Vec<ProjectCustomField>> {
        let mut standard = get_standard_custom_fields();

        // Try to fetch real project statuses and splice them into the "status" field
        if let Ok(groups) = self.list_project_statuses(project_id) {
            let (values, state_values) = crate::convert::flatten_project_statuses(&groups);
            if !values.is_empty()
                && let Some(status_field) = standard.iter_mut().find(|f| f.id == "status")
            {
                status_field.values = values;
                status_field.state_values = state_values;
            }
        }

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

    fn get_comments_page(&self, issue_id: &str, limit: usize, skip: usize) -> Result<Vec<Comment>> {
        Ok(self
            .get_comments_page(issue_id, limit, skip)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn get_issue_history(&self, issue_id: &str) -> Result<Vec<IssueHistoryEvent>> {
        let entries = self.get_issue_changelog(issue_id)?;
        Ok(jira_changelog_to_history_events(entries))
    }
}

/// Give up after this many consecutive cursor pages that yield no new
/// issues. Mid-stream pages may legitimately come back empty (or fully
/// filtered), but an unbounded run of them means the server is walking us
/// in circles — fail loudly instead of looping or silently truncating.
const MAX_NO_PROGRESS_PAGES: usize = 5;

/// If the query already looks like JQL, use it directly; otherwise convert
/// from the simple tracker-core query format.
fn to_jql(query: &str) -> String {
    if query.contains('=') || query.contains(" AND ") || query.contains(" OR ") {
        query.to_string()
    } else {
        convert_simple_query_to_jql(query)
    }
}

/// Convert simple tracker-core query format to JQL
fn convert_simple_query_to_jql(query: &str) -> String {
    let mut parts = Vec::new();
    let mut keywords = Vec::new();

    for token in query.split_whitespace() {
        if let Some(project) = token.strip_prefix("project:") {
            if !project.is_empty() {
                parts.push(format!("project = {}", project));
            }
        } else if let Some(state) = token.strip_prefix('#') {
            if state.eq_ignore_ascii_case("unresolved") {
                parts.push("resolution IS EMPTY".to_string());
            } else if state.eq_ignore_ascii_case("resolved") {
                parts.push("resolution IS NOT EMPTY".to_string());
            } else if state.eq_ignore_ascii_case("open") {
                parts.push("status = Open".to_string());
            } else if state.eq_ignore_ascii_case("closed") {
                parts.push("status = Closed".to_string());
            } else if state.eq_ignore_ascii_case("done") {
                parts.push("status = Done".to_string());
            } else if state.eq_ignore_ascii_case("inprogress")
                || state.eq_ignore_ascii_case("in-progress")
            {
                parts.push("status = \"In Progress\"".to_string());
            } else {
                parts.push(format!("status = \"{}\"", state));
            }
        } else {
            keywords.push(token);
        }
    }

    if !keywords.is_empty() {
        let joined_keywords = keywords.join(" ");
        parts.push(format!("text ~ \"{}\"", joined_keywords));
    }

    if parts.is_empty() {
        String::new()
    } else {
        parts.join(" AND ")
    }
}

fn jira_attachment_to_core(attachment: crate::models::JiraAttachment) -> IssueAttachment {
    let created = parse_jira_datetime(&attachment.created);

    IssueAttachment {
        id: attachment.id,
        name: attachment.filename,
        size: attachment.size,
        mime_type: attachment.mime_type,
        url: attachment.content,
        created,
        author: attachment.author.map(|author| {
            let login = author
                .account_id
                .clone()
                .or_else(|| author.display_name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            tracker_core::CommentAuthor {
                login,
                name: author.display_name,
            }
        }),
        comment_id: None,
        markdown: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_simple_query_to_jql_correct() {
        // Test 1: Mixed query preserves keywords under text operator
        let query = "project:PROJ #unresolved bug";
        let jql = convert_simple_query_to_jql(query);
        assert_eq!(
            jql,
            "project = PROJ AND resolution IS EMPTY AND text ~ \"bug\""
        );

        // Test 2: Pure keyword query converts cleanly to text operator JQL
        let pure_keyword = "bug fixing";
        let jql_pure_keyword = convert_simple_query_to_jql(pure_keyword);
        assert_eq!(jql_pure_keyword, "text ~ \"bug fixing\"");
    }

    #[test]
    fn attachment_created_parses_jira_cloud_offset() {
        use chrono::TimeZone;
        let attachment = crate::models::JiraAttachment {
            id: "10000".to_string(),
            filename: "log.txt".to_string(),
            size: 42,
            mime_type: None,
            content: None,
            created: Some("2024-01-15T10:00:00.000+0000".to_string()),
            author: None,
        };

        let core = jira_attachment_to_core(attachment);
        assert_eq!(
            core.created,
            Some(chrono::Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap())
        );
    }
}
