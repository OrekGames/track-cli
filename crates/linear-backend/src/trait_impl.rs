//! Implementation of tracker-core traits for LinearClient.

use tracker_core::{
    Article, ArticleAttachment, AttachmentUpload, Comment, CreateArticle, CreateIssue,
    CreateProject, CreateTag, CustomFieldUpdate, Issue, IssueHistoryEvent, IssueLink,
    IssueLinkType, IssueTag, IssueTracker, KnowledgeBase, Project, ProjectCustomField, Result,
    SearchResult, TrackerError, UpdateArticle, UpdateIssue, User,
};

use crate::client::LinearClient;
use crate::convert::{
    build_filter_from_parsed, linear_history_to_events, linear_issue_to_core, linear_link_types,
    linear_relations_to_core, parse_linear_query, priority_from_label, team_details_custom_fields,
};
use crate::models::{
    LinearIssueCreateInput, LinearIssueLabelCreateInput, LinearIssueLabelUpdateInput,
    LinearIssueRelationCreateInput, LinearIssueUpdateInput,
};

impl IssueTracker for LinearClient {
    fn get_issue(&self, id: &str) -> Result<Issue> {
        Ok(linear_issue_to_core(self.get_issue(id)?))
    }

    fn search_issues(&self, query: &str, limit: usize, skip: usize) -> Result<SearchResult<Issue>> {
        if limit == 0 {
            return Ok(SearchResult::from_items(Vec::new()));
        }

        let parsed = parse_linear_query(query);
        let team_id = if let Some(team) = parsed.team.as_deref() {
            Some(self.resolve_team_id(team)?)
        } else {
            None
        };
        let assignee_id = if parsed
            .assignee
            .as_deref()
            .is_some_and(|assignee| assignee.eq_ignore_ascii_case("me"))
        {
            Some(self.viewer()?.id)
        } else {
            None
        };
        let filter = build_filter_from_parsed(&parsed, team_id.as_deref(), assignee_id.as_deref());
        let term = if parsed.text.is_empty() {
            None
        } else {
            Some(parsed.text.join(" "))
        };

        let mut items = Vec::new();
        let mut total = None;
        let mut after = None;
        let mut remaining_skip = skip;

        while items.len() < limit {
            let (page, page_total, page_info) =
                self.search_issues_page(filter.clone(), term.as_deref(), 100, after)?;
            if total.is_none() {
                total = page_total;
            }

            let page_len = page.len();
            if remaining_skip >= page_len {
                remaining_skip -= page_len;
            } else {
                let remaining = limit - items.len();
                items.extend(
                    page.into_iter()
                        .skip(remaining_skip)
                        .take(remaining)
                        .map(linear_issue_to_core),
                );
                remaining_skip = 0;
            }

            if !page_info.has_next_page || page_len == 0 {
                break;
            }
            after = page_info.end_cursor;
        }

        match total {
            Some(total) => Ok(SearchResult::with_total(items, total)),
            None => Ok(SearchResult::from_items(items)),
        }
    }

    fn get_issue_count(&self, query: &str) -> Result<Option<u64>> {
        let parsed = parse_linear_query(query);
        if parsed.text.is_empty() {
            return Ok(None);
        }

        let team_id = if let Some(team) = parsed.team.as_deref() {
            Some(self.resolve_team_id(team)?)
        } else {
            None
        };
        let assignee_id = if parsed
            .assignee
            .as_deref()
            .is_some_and(|assignee| assignee.eq_ignore_ascii_case("me"))
        {
            Some(self.viewer()?.id)
        } else {
            None
        };
        let filter = build_filter_from_parsed(&parsed, team_id.as_deref(), assignee_id.as_deref());
        let term = parsed.text.join(" ");
        let (_issues, total, _page_info) = self.search_issues_page(filter, Some(&term), 1, None)?;
        Ok(total)
    }

    fn create_issue(&self, issue: &CreateIssue) -> Result<Issue> {
        let input = self.build_create_input(issue)?;
        Ok(linear_issue_to_core(self.create_issue(&input)?))
    }

    fn update_issue(&self, id: &str, update: &UpdateIssue) -> Result<Issue> {
        let existing = self.get_issue(id)?;
        let input = self.build_update_input(&existing.team.id, update)?;
        Ok(linear_issue_to_core(self.update_issue(id, &input)?))
    }

    fn delete_issue(&self, id: &str) -> Result<()> {
        Ok(self.delete_issue(id)?)
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        Ok(self.list_teams()?.into_iter().map(Into::into).collect())
    }

    fn get_project(&self, id: &str) -> Result<Project> {
        Ok(self.get_team(id)?.into())
    }

    fn create_project(&self, _project: &CreateProject) -> Result<Project> {
        Err(TrackerError::InvalidInput(
            "Creating Linear teams via this tool is not supported. Create the team in Linear first."
                .to_string(),
        ))
    }

    fn resolve_project_id(&self, identifier: &str) -> Result<String> {
        Ok(self.resolve_team_id(identifier)?)
    }

    fn get_project_custom_fields(&self, project_id: &str) -> Result<Vec<ProjectCustomField>> {
        let details = self.get_team_details(project_id)?;
        Ok(team_details_custom_fields(&details))
    }

    fn list_project_users(&self, project_id: &str) -> Result<Vec<User>> {
        Ok(self
            .list_team_members(project_id)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn list_tags(&self) -> Result<Vec<IssueTag>> {
        let labels = if let Some(team) = self.default_team() {
            let team_id = self.resolve_team_id(team)?;
            self.list_team_labels(&team_id)?
        } else {
            self.list_issue_labels(None)?
        };
        Ok(labels.into_iter().map(Into::into).collect())
    }

    fn create_tag(&self, tag: &CreateTag) -> Result<IssueTag> {
        let team_id = self.default_team_id_for_tag_mutation()?;
        let input = LinearIssueLabelCreateInput {
            team_id,
            name: tag.name.clone(),
            color: tag.color.clone(),
            description: tag.description.clone(),
        };
        Ok(self.create_label(&input)?.into())
    }

    fn delete_tag(&self, name: &str) -> Result<()> {
        let team_id = self.default_team_id_for_tag_mutation()?;
        let label = self.find_label(&team_id, name)?;
        Ok(self.delete_label(&label.id)?)
    }

    fn update_tag(&self, current_name: &str, tag: &CreateTag) -> Result<IssueTag> {
        let team_id = self.default_team_id_for_tag_mutation()?;
        let label = self.find_label(&team_id, current_name)?;
        let input = LinearIssueLabelUpdateInput {
            name: (tag.name != current_name).then(|| tag.name.clone()),
            color: tag.color.clone(),
            description: tag.description.clone(),
        };
        Ok(self.update_label(&label.id, &input)?.into())
    }

    fn list_link_types(&self) -> Result<Vec<IssueLinkType>> {
        Ok(linear_link_types())
    }

    fn get_issue_links(&self, issue_id: &str) -> Result<Vec<IssueLink>> {
        let issue = self.get_issue(issue_id)?;
        let relations = self.get_issue_relations(&issue.id)?;
        Ok(linear_relations_to_core(&issue.id, relations))
    }

    fn link_issues(
        &self,
        source: &str,
        target: &str,
        link_type: &str,
        direction: &str,
    ) -> Result<()> {
        let source_issue = self.get_issue(source)?;
        let target_issue = self.get_issue(target)?;
        let linear_type = self.resolve_link_type(link_type);

        let (issue_id, related_issue_id) = match (link_type, direction) {
            ("depends", _) => (&target_issue.id, &source_issue.id),
            ("required", _) => (&source_issue.id, &target_issue.id),
            ("duplicated-by", _) => (&target_issue.id, &source_issue.id),
            (_, "INWARD") if linear_type == "blocks" => (&target_issue.id, &source_issue.id),
            _ => (&source_issue.id, &target_issue.id),
        };

        let input = LinearIssueRelationCreateInput {
            issue_id: issue_id.clone(),
            related_issue_id: related_issue_id.clone(),
            relation_type: linear_type,
        };
        self.create_issue_relation(&input)?;
        Ok(())
    }

    fn link_subtask(&self, child: &str, parent: &str) -> Result<()> {
        let parent_issue = self.get_issue(parent)?;
        let update = LinearIssueUpdateInput {
            parent_id: Some(Some(parent_issue.id)),
            ..Default::default()
        };
        self.update_issue(child, &update)?;
        Ok(())
    }

    fn unlink_issues(&self, source: &str, link_id: &str) -> Result<()> {
        if link_id.starts_with("linear-parent:") {
            let update = LinearIssueUpdateInput {
                parent_id: Some(None),
                ..Default::default()
            };
            self.update_issue(source, &update)?;
            return Ok(());
        }

        if let Some(child_id) = link_id.strip_prefix("linear-child:") {
            let update = LinearIssueUpdateInput {
                parent_id: Some(None),
                ..Default::default()
            };
            self.update_issue(child_id, &update)?;
            return Ok(());
        }

        Ok(self.delete_issue_relation(link_id)?)
    }

    fn add_comment(&self, issue_id: &str, text: &str) -> Result<Comment> {
        let issue = self.get_issue(issue_id)?;
        Ok(self.add_comment(&issue.id, text)?.into())
    }

    fn get_comments(&self, issue_id: &str) -> Result<Vec<Comment>> {
        <Self as IssueTracker>::get_comments_page(self, issue_id, 100, 0)
    }

    fn get_comments_page(&self, issue_id: &str, limit: usize, skip: usize) -> Result<Vec<Comment>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let issue = self.get_issue(issue_id)?;
        let mut comments = Vec::new();
        let mut remaining_skip = skip;
        let mut after = None;

        while comments.len() < limit {
            let (page, page_info) = self.get_comments_page(&issue.id, 100, after)?;
            let page_len = page.len();

            if remaining_skip >= page_len {
                remaining_skip -= page_len;
            } else {
                let remaining = limit - comments.len();
                comments.extend(
                    page.into_iter()
                        .skip(remaining_skip)
                        .take(remaining)
                        .map(Into::into),
                );
                remaining_skip = 0;
            }

            if !page_info.has_next_page || page_len == 0 {
                break;
            }
            after = page_info.end_cursor;
        }

        Ok(comments)
    }

    fn get_issue_history(&self, issue_id: &str) -> Result<Vec<IssueHistoryEvent>> {
        // Generous backstop against a server that keeps reporting `hasNextPage`
        // without advancing the cursor; large enough never to truncate a real
        // history (100 nodes/page).
        const MAX_PAGES: usize = 10_000;

        // Resolve the readable identifier to Linear's internal id, exactly as
        // the comments path does, since `Issue.history` keys on the id.
        let issue = self.get_issue(issue_id)?;
        let mut nodes = Vec::new();
        let mut after = None;

        for _ in 0..MAX_PAGES {
            let (page, page_info) = self.get_issue_history_page(&issue.id, 100, after)?;
            let page_len = page.len();
            nodes.extend(page);

            if !page_info.has_next_page || page_len == 0 {
                return Ok(linear_history_to_events(nodes));
            }
            after = page_info.end_cursor;
        }

        Err(TrackerError::PaginationStalled(format!(
            "issue '{}' history did not terminate after {} pages",
            issue_id, MAX_PAGES
        )))
    }
}

impl LinearClient {
    fn build_create_input(&self, issue: &CreateIssue) -> Result<LinearIssueCreateInput> {
        let team_id = issue.project_id.clone();
        let mut input = LinearIssueCreateInput {
            title: issue.summary.clone(),
            team_id: team_id.clone(),
            description: issue.description.clone(),
            ..Default::default()
        };

        self.apply_custom_fields_to_create(&team_id, &issue.custom_fields, &mut input)?;

        if !issue.tags.is_empty() {
            input.label_ids = Some(self.resolve_label_ids(&team_id, &issue.tags)?);
        }

        if input.project_id.is_none()
            && let Some(default_project) = self.default_linear_project()
        {
            input.project_id = Some(self.find_project(&team_id, default_project)?.id);
        }

        if let Some(parent) = issue.parent.as_deref() {
            input.parent_id = Some(self.get_issue(parent)?.id);
        }

        Ok(input)
    }

    fn build_update_input(
        &self,
        team_id: &str,
        update: &UpdateIssue,
    ) -> Result<LinearIssueUpdateInput> {
        let mut input = LinearIssueUpdateInput {
            title: update.summary.clone(),
            description: update.description.clone(),
            ..Default::default()
        };

        self.apply_custom_fields_to_update(team_id, &update.custom_fields, &mut input)?;

        if !update.tags.is_empty() {
            input.label_ids = Some(self.resolve_label_ids(team_id, &update.tags)?);
        }

        if let Some(parent) = update.parent.as_deref() {
            input.parent_id = Some(Some(self.get_issue(parent)?.id));
        }

        Ok(input)
    }

    fn apply_custom_fields_to_create(
        &self,
        team_id: &str,
        fields: &[CustomFieldUpdate],
        input: &mut LinearIssueCreateInput,
    ) -> Result<()> {
        for field in fields {
            match field {
                CustomFieldUpdate::State { name, value } if is_state_field(name) => {
                    input.state_id = Some(self.resolve_state_id(team_id, value)?);
                }
                CustomFieldUpdate::SingleEnum { name, value }
                    if name.eq_ignore_ascii_case("priority") =>
                {
                    input.priority = Some(priority_from_label(value).ok_or_else(|| {
                        TrackerError::InvalidInput(format!(
                            "Invalid Linear priority '{}'. Valid values: No priority, Urgent, High, Medium, Low",
                            value
                        ))
                    })?);
                }
                CustomFieldUpdate::SingleUser { name, login }
                    if name.eq_ignore_ascii_case("assignee") =>
                {
                    input.assignee_id = Some(self.find_user(team_id, login)?.id);
                }
                CustomFieldUpdate::SingleEnum { name, value }
                    if name.eq_ignore_ascii_case("project") =>
                {
                    input.project_id = Some(self.find_project(team_id, value)?.id);
                }
                CustomFieldUpdate::MultiEnum { name, values }
                    if name.eq_ignore_ascii_case("labels") || name.eq_ignore_ascii_case("tags") =>
                {
                    input.label_ids = Some(self.resolve_label_ids(team_id, values)?);
                }
                CustomFieldUpdate::SingleEnum { name, value }
                    if name.eq_ignore_ascii_case("label")
                        || name.eq_ignore_ascii_case("labels")
                        || name.eq_ignore_ascii_case("tag") =>
                {
                    input.label_ids =
                        Some(self.resolve_label_ids(team_id, std::slice::from_ref(value))?);
                }
                other => return Err(unsupported_linear_field(other)),
            }
        }
        Ok(())
    }

    fn apply_custom_fields_to_update(
        &self,
        team_id: &str,
        fields: &[CustomFieldUpdate],
        input: &mut LinearIssueUpdateInput,
    ) -> Result<()> {
        for field in fields {
            match field {
                CustomFieldUpdate::State { name, value } if is_state_field(name) => {
                    input.state_id = Some(self.resolve_state_id(team_id, value)?);
                }
                CustomFieldUpdate::SingleEnum { name, value }
                    if name.eq_ignore_ascii_case("priority") =>
                {
                    input.priority = Some(priority_from_label(value).ok_or_else(|| {
                        TrackerError::InvalidInput(format!(
                            "Invalid Linear priority '{}'. Valid values: No priority, Urgent, High, Medium, Low",
                            value
                        ))
                    })?);
                }
                CustomFieldUpdate::SingleUser { name, login }
                    if name.eq_ignore_ascii_case("assignee") =>
                {
                    input.assignee_id = Some(self.find_user(team_id, login)?.id);
                }
                CustomFieldUpdate::SingleEnum { name, value }
                    if name.eq_ignore_ascii_case("project") =>
                {
                    input.project_id = Some(self.find_project(team_id, value)?.id);
                }
                CustomFieldUpdate::MultiEnum { name, values }
                    if name.eq_ignore_ascii_case("labels") || name.eq_ignore_ascii_case("tags") =>
                {
                    input.label_ids = Some(self.resolve_label_ids(team_id, values)?);
                }
                CustomFieldUpdate::SingleEnum { name, value }
                    if name.eq_ignore_ascii_case("label")
                        || name.eq_ignore_ascii_case("labels")
                        || name.eq_ignore_ascii_case("tag") =>
                {
                    input.label_ids =
                        Some(self.resolve_label_ids(team_id, std::slice::from_ref(value))?);
                }
                other => return Err(unsupported_linear_field(other)),
            }
        }
        Ok(())
    }

    fn resolve_state_id(&self, team_id: &str, value: &str) -> Result<String> {
        let states = self.list_team_states(team_id)?;
        if let Some(state) = states
            .iter()
            .find(|state| state.name.eq_ignore_ascii_case(value))
        {
            return Ok(state.id.clone());
        }

        let lower = value.to_lowercase();
        let target_type = if matches!(
            lower.as_str(),
            "develop" | "started" | "start" | "in progress" | "progress"
        ) {
            Some("started")
        } else if matches!(
            lower.as_str(),
            "done" | "complete" | "completed" | "resolved" | "closed"
        ) {
            Some("completed")
        } else {
            None
        };

        if let Some(target_type) = target_type
            && let Some(state) = states.iter().find(|state| state.state_type == target_type)
        {
            return Ok(state.id.clone());
        }

        let names = states
            .iter()
            .map(|state| state.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Err(TrackerError::InvalidInput(format!(
            "Invalid Linear state '{}'. Valid states: {}",
            value, names
        )))
    }

    fn resolve_label_ids(&self, team_id: &str, labels: &[String]) -> Result<Vec<String>> {
        labels
            .iter()
            .map(|name| Ok(self.find_label(team_id, name)?.id))
            .collect()
    }

    fn default_team_id_for_tag_mutation(&self) -> Result<String> {
        let default_team = self.default_team().ok_or_else(|| {
            TrackerError::InvalidInput(
                "Linear tag create/update/delete requires a configured default team. Set default_project or linear.default_team.".to_string(),
            )
        })?;
        Ok(self.resolve_team_id(default_team)?)
    }
}

impl KnowledgeBase for LinearClient {
    fn get_article(&self, _id: &str) -> Result<Article> {
        Err(linear_articles_unsupported())
    }

    fn list_articles(
        &self,
        _project_id: Option<&str>,
        _limit: usize,
        _skip: usize,
    ) -> Result<Vec<Article>> {
        Ok(Vec::new())
    }

    fn search_articles(&self, _query: &str, _limit: usize, _skip: usize) -> Result<Vec<Article>> {
        Ok(Vec::new())
    }

    fn create_article(&self, _article: &CreateArticle) -> Result<Article> {
        Err(linear_articles_unsupported())
    }

    fn update_article(&self, _id: &str, _update: &UpdateArticle) -> Result<Article> {
        Err(linear_articles_unsupported())
    }

    fn delete_article(&self, _id: &str) -> Result<()> {
        Err(linear_articles_unsupported())
    }

    fn get_child_articles(&self, _parent_id: &str) -> Result<Vec<Article>> {
        Ok(Vec::new())
    }

    fn move_article(&self, _article_id: &str, _new_parent_id: Option<&str>) -> Result<Article> {
        Err(linear_articles_unsupported())
    }

    fn list_article_attachments(&self, _article_id: &str) -> Result<Vec<ArticleAttachment>> {
        Err(linear_articles_unsupported())
    }

    fn add_article_attachment(
        &self,
        _article_id: &str,
        _upload: &AttachmentUpload,
    ) -> Result<Vec<ArticleAttachment>> {
        Err(linear_articles_unsupported())
    }

    fn get_article_comments(&self, _article_id: &str) -> Result<Vec<Comment>> {
        Ok(Vec::new())
    }

    fn add_article_comment(&self, _article_id: &str, _text: &str) -> Result<Comment> {
        Err(linear_articles_unsupported())
    }
}

fn is_state_field(name: &str) -> bool {
    name.eq_ignore_ascii_case("status")
        || name.eq_ignore_ascii_case("state")
        || name.eq_ignore_ascii_case("stage")
}

fn unsupported_linear_field(field: &CustomFieldUpdate) -> TrackerError {
    let name = match field {
        CustomFieldUpdate::SingleEnum { name, .. }
        | CustomFieldUpdate::MultiEnum { name, .. }
        | CustomFieldUpdate::State { name, .. }
        | CustomFieldUpdate::SingleUser { name, .. } => name,
    };
    TrackerError::InvalidInput(format!(
        "Linear does not support updating field '{}'. Supported fields: Status/State/Stage, Assignee, Priority, Labels, Project.",
        name
    ))
}

fn linear_articles_unsupported() -> TrackerError {
    TrackerError::InvalidInput("Linear knowledge base articles are not supported".to_string())
}
