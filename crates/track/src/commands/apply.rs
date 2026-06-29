use crate::cli::OutputFormat;
use crate::commands::issue;
use crate::output::output_json;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use tracker_core::{CreateIssue, CustomFieldUpdate, Issue, IssueTracker, UpdateIssue};

pub(crate) struct ApplyOptions<'a> {
    pub(crate) plan_path: &'a Path,
    pub(crate) dry_run: bool,
    pub(crate) validate: bool,
    pub(crate) resume_path: Option<&'a Path>,
    pub(crate) allow_delete: bool,
    pub(crate) format: OutputFormat,
    pub(crate) default_project: Option<&'a str>,
}

pub fn handle_apply(client: &dyn IssueTracker, options: ApplyOptions<'_>) -> Result<()> {
    let raw_plan = read_plan_bytes(options.plan_path)?;
    let checksum = plan_checksum(&raw_plan);
    let plan = parse_apply_plan(&raw_plan)?;
    let (state, resumed) = load_resume_state(options.resume_path, &checksum)?;

    if let Err(failure) = validate_reference_order(&plan) {
        let output = failure_output(&plan, options.dry_run, resumed, state.refs, failure);
        output_apply_result(&output, options.format)?;
        return Err(anyhow!(first_error(&output)));
    }

    if !options.dry_run
        && !options.allow_delete
        && let Some((index, op)) = plan
            .operations
            .iter()
            .enumerate()
            .find(|(_, op)| matches!(op, ApplyOperation::DeleteIssue { .. }))
    {
        let output = failure_output(
            &plan,
            options.dry_run,
            resumed,
            state.refs,
            PreflightFailure {
                index,
                op: op.op_name().to_string(),
                message: "delete_issue operations require --allow-delete".to_string(),
            },
        );
        output_apply_result(&output, options.format)?;
        return Err(anyhow!(first_error(&output)));
    }

    let execution = execute_plan(ApplyExecution {
        client,
        plan: &plan,
        checksum,
        dry_run: options.dry_run,
        validate: options.validate,
        resume_path: options.resume_path,
        default_project: options.default_project,
        resumed,
        state,
    })?;

    output_apply_result(&execution.output, options.format)?;
    if execution.output.success {
        Ok(())
    } else {
        Err(anyhow!(
            execution
                .error
                .unwrap_or_else(|| "apply failed".to_string())
        ))
    }
}

#[derive(Debug, Deserialize)]
struct ApplyPlan {
    version: u32,
    #[serde(default)]
    defaults: ApplyDefaults,
    operations: Vec<ApplyOperation>,
}

#[derive(Debug, Default, Deserialize)]
struct ApplyDefaults {
    project: Option<String>,
    validate: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum ApplyOperation {
    CreateIssue {
        #[serde(default, rename = "ref")]
        ref_name: Option<String>,
        #[serde(default)]
        project: Option<String>,
        summary: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        fields: BTreeMap<String, PlanFieldValue>,
        #[serde(default, rename = "customFields")]
        custom_fields: Vec<serde_json::Value>,
        #[serde(default)]
        state: Option<String>,
        #[serde(default)]
        priority: Option<String>,
        #[serde(default)]
        assignee: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        parent: Option<String>,
        #[serde(default)]
        dedupe: Option<Dedupe>,
    },
    UpdateIssue {
        issue: String,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        fields: BTreeMap<String, PlanFieldValue>,
        #[serde(default, rename = "customFields")]
        custom_fields: Vec<serde_json::Value>,
        #[serde(default)]
        state: Option<String>,
        #[serde(default)]
        priority: Option<String>,
        #[serde(default)]
        assignee: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        parent: Option<String>,
    },
    Comment {
        issue: String,
        body: String,
    },
    Link {
        source: String,
        target: String,
        #[serde(default = "default_link_type", rename = "type")]
        link_type: String,
    },
    DeleteIssue {
        issue: String,
    },
}

impl ApplyOperation {
    fn op_name(&self) -> &'static str {
        match self {
            Self::CreateIssue { .. } => "create_issue",
            Self::UpdateIssue { .. } => "update_issue",
            Self::Comment { .. } => "comment",
            Self::Link { .. } => "link",
            Self::DeleteIssue { .. } => "delete_issue",
        }
    }

    fn defined_ref(&self) -> Option<&str> {
        match self {
            Self::CreateIssue {
                ref_name: Some(ref_name),
                ..
            } => Some(ref_name),
            _ => None,
        }
    }

    fn referenced_values(&self) -> Vec<&str> {
        match self {
            Self::CreateIssue { parent, .. } => parent.iter().map(String::as_str).collect(),
            Self::UpdateIssue { issue, parent, .. } => {
                let mut refs = vec![issue.as_str()];
                if let Some(parent) = parent {
                    refs.push(parent.as_str());
                }
                refs
            }
            Self::Comment { issue, .. } => vec![issue.as_str()],
            Self::Link { source, target, .. } => vec![source.as_str(), target.as_str()],
            Self::DeleteIssue { issue } => vec![issue.as_str()],
        }
    }
}

#[derive(Debug, Deserialize)]
struct Dedupe {
    query: String,
    on_match: DedupeAction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DedupeAction {
    Reuse,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PlanFieldValue {
    Single(String),
    Multi(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplyResumeState {
    version: u32,
    plan_checksum: String,
    #[serde(default)]
    completed: BTreeSet<usize>,
    #[serde(default)]
    refs: BTreeMap<String, String>,
    #[serde(default)]
    results: Vec<ApplyOperationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplyOperationResult {
    index: usize,
    op: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    issue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "ref")]
    ref_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
}

impl ApplyOperationResult {
    fn success(index: usize, op: &str, status: &str) -> Self {
        Self {
            index,
            op: op.to_string(),
            status: status.to_string(),
            issue: None,
            ref_name: None,
            error: None,
            warnings: Vec::new(),
        }
    }

    fn failed(index: usize, op: &str, error: impl Into<String>) -> Self {
        Self {
            index,
            op: op.to_string(),
            status: "failed".to_string(),
            issue: None,
            ref_name: None,
            error: Some(error.into()),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ApplyOutput {
    success: bool,
    dry_run: bool,
    resumed: bool,
    summary: ApplySummary,
    refs: BTreeMap<String, String>,
    results: Vec<ApplyOperationResult>,
}

#[derive(Debug, Serialize)]
struct ApplySummary {
    total: usize,
    by_status: BTreeMap<String, usize>,
    by_op: BTreeMap<String, usize>,
}

struct ApplyExecution<'a> {
    client: &'a dyn IssueTracker,
    plan: &'a ApplyPlan,
    checksum: String,
    dry_run: bool,
    validate: bool,
    resume_path: Option<&'a Path>,
    default_project: Option<&'a str>,
    resumed: bool,
    state: ApplyResumeState,
}

#[derive(Clone, Copy)]
struct OperationContext<'a> {
    client: &'a dyn IssueTracker,
    plan: &'a ApplyPlan,
    dry_run: bool,
    validate: bool,
    default_project: Option<&'a str>,
}

struct ExecutionResult {
    output: ApplyOutput,
    error: Option<String>,
}

#[derive(Debug)]
struct PreflightFailure {
    index: usize,
    op: String,
    message: String,
}

fn execute_plan(execution: ApplyExecution<'_>) -> Result<ExecutionResult> {
    let mut refs = execution.state.refs.clone();
    let mut completed = execution.state.completed.clone();
    let mut state_results: BTreeMap<usize, ApplyOperationResult> = execution
        .state
        .results
        .into_iter()
        .map(|result| (result.index, result))
        .collect();
    let mut output_results = Vec::new();
    let operation_context = OperationContext {
        client: execution.client,
        plan: execution.plan,
        dry_run: execution.dry_run,
        validate: execution.validate,
        default_project: execution.default_project,
    };

    for (index, operation) in execution.plan.operations.iter().enumerate() {
        if completed.contains(&index) {
            let mut result = state_results.get(&index).cloned().unwrap_or_else(|| {
                ApplyOperationResult::success(index, operation.op_name(), "skipped")
            });
            result.status = "skipped".to_string();
            output_results.push(result);
            continue;
        }

        match execute_operation(operation_context, operation, index, &mut refs) {
            Ok(result) => {
                if !execution.dry_run {
                    completed.insert(index);
                    state_results.insert(index, result.clone());
                    write_resume_state(
                        execution.resume_path,
                        &execution.checksum,
                        &completed,
                        &refs,
                        &state_results,
                    )?;
                }
                output_results.push(result);
            }
            Err(error) => {
                let message = error.to_string();
                output_results.push(ApplyOperationResult::failed(
                    index,
                    operation.op_name(),
                    message.clone(),
                ));
                let output = build_output(
                    false,
                    execution.dry_run,
                    execution.resumed,
                    execution.plan.operations.len(),
                    refs,
                    output_results,
                );
                return Ok(ExecutionResult {
                    output,
                    error: Some(message),
                });
            }
        }
    }

    let output = build_output(
        true,
        execution.dry_run,
        execution.resumed,
        execution.plan.operations.len(),
        refs,
        output_results,
    );

    Ok(ExecutionResult {
        output,
        error: None,
    })
}

fn execute_operation(
    context: OperationContext<'_>,
    operation: &ApplyOperation,
    index: usize,
    refs: &mut BTreeMap<String, String>,
) -> Result<ApplyOperationResult> {
    match operation {
        ApplyOperation::CreateIssue {
            ref_name,
            project,
            summary,
            description,
            fields,
            custom_fields,
            state,
            priority,
            assignee,
            tags,
            parent,
            dedupe,
        } => execute_create_issue(
            context.client,
            context.plan,
            index,
            context.dry_run,
            context.validate,
            context.default_project,
            refs,
            ref_name.as_deref(),
            project.as_deref(),
            summary,
            description.as_deref(),
            fields,
            custom_fields,
            state.as_deref(),
            priority.as_deref(),
            assignee.as_deref(),
            tags,
            parent.as_deref(),
            dedupe.as_ref(),
        ),
        ApplyOperation::UpdateIssue {
            issue,
            summary,
            description,
            fields,
            custom_fields,
            state,
            priority,
            assignee,
            tags,
            parent,
        } => execute_update_issue(
            context.client,
            context.plan,
            index,
            context.dry_run,
            context.validate,
            refs,
            issue,
            summary.as_deref(),
            description.as_deref(),
            fields,
            custom_fields,
            state.as_deref(),
            priority.as_deref(),
            assignee.as_deref(),
            tags,
            parent.as_deref(),
        ),
        ApplyOperation::Comment { issue, body } => {
            let issue_id = resolve_issue_ref(issue, refs)?;
            let mut result = ApplyOperationResult::success(
                index,
                operation.op_name(),
                if context.dry_run {
                    "dry_run"
                } else {
                    "commented"
                },
            );
            result.issue = Some(issue_id.clone());
            if !context.dry_run {
                context
                    .client
                    .add_comment(&issue_id, body)
                    .with_context(|| format!("Failed to comment on '{}'", issue_id))?;
            }
            Ok(result)
        }
        ApplyOperation::Link {
            source,
            target,
            link_type,
        } => {
            let source_id = resolve_issue_ref(source, refs)?;
            let target_id = resolve_issue_ref(target, refs)?;
            let mut result = ApplyOperationResult::success(
                index,
                operation.op_name(),
                if context.dry_run { "dry_run" } else { "linked" },
            );
            result.issue = Some(source_id.clone());
            if !context.dry_run {
                issue::link_issues_with_type(context.client, &source_id, &target_id, link_type)?;
            }
            Ok(result)
        }
        ApplyOperation::DeleteIssue { issue } => {
            let issue_id = resolve_issue_ref(issue, refs)?;
            let mut result = ApplyOperationResult::success(
                index,
                operation.op_name(),
                if context.dry_run {
                    "dry_run"
                } else {
                    "deleted"
                },
            );
            result.issue = Some(issue_id.clone());
            if !context.dry_run {
                context
                    .client
                    .delete_issue(&issue_id)
                    .with_context(|| format!("Failed to delete '{}'", issue_id))?;
            }
            Ok(result)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_create_issue(
    client: &dyn IssueTracker,
    plan: &ApplyPlan,
    index: usize,
    dry_run: bool,
    validate: bool,
    default_project: Option<&str>,
    refs: &mut BTreeMap<String, String>,
    ref_name: Option<&str>,
    project: Option<&str>,
    summary: &str,
    description: Option<&str>,
    fields: &BTreeMap<String, PlanFieldValue>,
    raw_custom_fields: &[serde_json::Value],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    tags: &[String],
    parent: Option<&str>,
    dedupe: Option<&Dedupe>,
) -> Result<ApplyOperationResult> {
    let project_input = project
        .or(plan.defaults.project.as_deref())
        .or(default_project)
        .ok_or_else(|| {
            anyhow!(
                "Project is required for create_issue. Set operation.project, defaults.project, or config default_project."
            )
        })?;

    let project_id = client
        .resolve_project_id(project_input)
        .with_context(|| format!("Failed to resolve project '{}'", project_input))?;
    let schema = load_schema_for_detection(client, &project_id, !fields.is_empty());
    let custom_fields = build_custom_field_updates(
        fields,
        raw_custom_fields,
        state,
        priority,
        assignee,
        schema.as_deref(),
    )?;
    let effective_validate = validate || plan.defaults.validate.unwrap_or(false);
    if effective_validate {
        issue::validate_custom_fields(client, &project_id, &custom_fields)?;
    }

    if let Some(dedupe) = dedupe {
        let matches = dedupe_matches(client, dedupe)?;
        match matches.as_slice() {
            [] => {}
            [existing] => {
                let issue_id = issue_output_id(existing);
                if let Some(ref_name) = ref_name {
                    refs.insert(ref_name.to_string(), issue_id.clone());
                }
                let mut result = ApplyOperationResult::success(index, "create_issue", "reused");
                result.issue = Some(issue_id);
                result.ref_name = ref_name.map(String::from);
                return Ok(result);
            }
            _ => {
                let candidates = matches
                    .iter()
                    .map(issue_output_id)
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!(
                    "Dedupe query matched multiple issues; refusing to create. Candidates: {}",
                    candidates
                );
            }
        }
    }

    let parent = parent
        .map(|parent| resolve_issue_ref(parent, refs))
        .transpose()?;
    let create = CreateIssue {
        project_id,
        summary: summary.to_string(),
        description: description.map(String::from),
        custom_fields,
        tags: tags.to_vec(),
        parent,
    };

    if dry_run {
        let issue_id = ref_name.map(planned_ref_value);
        if let (Some(ref_name), Some(issue_id)) = (ref_name, issue_id.as_ref()) {
            refs.insert(ref_name.to_string(), issue_id.clone());
        }
        let mut result = ApplyOperationResult::success(index, "create_issue", "dry_run");
        result.issue = issue_id;
        result.ref_name = ref_name.map(String::from);
        return Ok(result);
    }

    let issue = client
        .create_issue(&create)
        .context("Failed to create issue")?;
    let issue_id = issue_output_id(&issue);
    if let Some(ref_name) = ref_name {
        refs.insert(ref_name.to_string(), issue_id.clone());
    }

    let mut result = ApplyOperationResult::success(index, "create_issue", "created");
    result.issue = Some(issue_id);
    result.ref_name = ref_name.map(String::from);
    result.warnings = issue::verify_issue_create(&create, &issue);
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn execute_update_issue(
    client: &dyn IssueTracker,
    plan: &ApplyPlan,
    index: usize,
    dry_run: bool,
    validate: bool,
    refs: &BTreeMap<String, String>,
    issue_ref: &str,
    summary: Option<&str>,
    description: Option<&str>,
    fields: &BTreeMap<String, PlanFieldValue>,
    raw_custom_fields: &[serde_json::Value],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    tags: &[String],
    parent: Option<&str>,
) -> Result<ApplyOperationResult> {
    let issue_id = resolve_issue_ref(issue_ref, refs)?;
    let effective_validate = validate || plan.defaults.validate.unwrap_or(false);
    let existing = if !fields.is_empty()
        || (effective_validate
            && has_any_custom_field_input(fields, raw_custom_fields, state, priority, assignee))
    {
        Some(
            client
                .get_issue(&issue_id)
                .with_context(|| format!("Failed to fetch issue '{}' for validation", issue_id))?,
        )
    } else {
        None
    };
    let schema = existing.as_ref().and_then(|existing| {
        load_schema_for_detection(client, &existing.project.id, !fields.is_empty())
    });
    let custom_fields = build_custom_field_updates(
        fields,
        raw_custom_fields,
        state,
        priority,
        assignee,
        schema.as_deref(),
    )?;
    let parent = parent
        .map(|parent| resolve_issue_ref(parent, refs))
        .transpose()?;

    if summary.is_none()
        && description.is_none()
        && custom_fields.is_empty()
        && tags.is_empty()
        && parent.is_none()
    {
        bail!("update_issue operation must include at least one field to update");
    }

    if effective_validate && !custom_fields.is_empty() {
        let existing = match existing {
            Some(existing) => existing,
            None => client
                .get_issue(&issue_id)
                .with_context(|| format!("Failed to fetch issue '{}' for validation", issue_id))?,
        };
        issue::validate_custom_fields(client, &existing.project.id, &custom_fields)?;
    }

    let update = UpdateIssue {
        summary: summary.map(String::from),
        description: description.map(String::from),
        custom_fields,
        tags: tags.to_vec(),
        parent,
    };

    let mut result = ApplyOperationResult::success(
        index,
        "update_issue",
        if dry_run { "dry_run" } else { "updated" },
    );
    result.issue = Some(issue_id.clone());

    if dry_run {
        return Ok(result);
    }

    let updated = client
        .update_issue(&issue_id, &update)
        .with_context(|| format!("Failed to update issue '{}'", issue_id))?;
    result.issue = Some(issue_output_id(&updated));
    result.warnings = issue::verify_issue_update(&update, &updated);
    Ok(result)
}

fn build_custom_field_updates(
    fields: &BTreeMap<String, PlanFieldValue>,
    raw_custom_fields: &[serde_json::Value],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    schema: Option<&[tracker_core::ProjectCustomField]>,
) -> Result<Vec<CustomFieldUpdate>> {
    let mut updates = Vec::new();

    for (name, value) in fields {
        match value {
            PlanFieldValue::Single(value) => updates.push(issue::build_custom_field_update(
                name.clone(),
                vec![value.clone()],
                false,
                schema,
            )),
            PlanFieldValue::Multi(values) => {
                if values.is_empty() {
                    bail!("Field '{}' array cannot be empty", name);
                }
                updates.push(issue::build_custom_field_update(
                    name.clone(),
                    values.clone(),
                    true,
                    schema,
                ));
            }
        }
    }

    updates.extend(issue::parse_custom_fields_json(raw_custom_fields)?);

    if let Some(state) = state {
        updates.push(CustomFieldUpdate::State {
            name: "State".to_string(),
            value: state.to_string(),
        });
    }
    if let Some(priority) = priority {
        updates.push(CustomFieldUpdate::SingleEnum {
            name: "Priority".to_string(),
            value: priority.to_string(),
        });
    }
    if let Some(assignee) = assignee {
        updates.push(CustomFieldUpdate::SingleUser {
            name: "Assignee".to_string(),
            login: assignee.to_string(),
        });
    }

    Ok(updates)
}

fn has_any_custom_field_input(
    fields: &BTreeMap<String, PlanFieldValue>,
    raw_custom_fields: &[serde_json::Value],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
) -> bool {
    !fields.is_empty()
        || !raw_custom_fields.is_empty()
        || state.is_some()
        || priority.is_some()
        || assignee.is_some()
}

fn dedupe_matches(client: &dyn IssueTracker, dedupe: &Dedupe) -> Result<Vec<Issue>> {
    match dedupe.on_match {
        DedupeAction::Reuse => {}
    }
    Ok(client
        .search_issues(&dedupe.query, 2, 0)
        .with_context(|| format!("Failed to run dedupe query '{}'", dedupe.query))?
        .items)
}

fn load_schema_for_detection(
    client: &dyn IssueTracker,
    project_id: &str,
    should_load: bool,
) -> Option<Vec<tracker_core::ProjectCustomField>> {
    if should_load {
        client.get_project_custom_fields(project_id).ok()
    } else {
        None
    }
}

fn parse_apply_plan(raw_plan: &[u8]) -> Result<ApplyPlan> {
    let plan: ApplyPlan = serde_json::from_slice(raw_plan).context("Invalid JSON apply plan")?;
    if plan.version != 1 {
        bail!(
            "Unsupported apply plan version {}. Expected version 1.",
            plan.version
        );
    }
    Ok(plan)
}

fn read_plan_bytes(path: &Path) -> Result<Vec<u8>> {
    if path.as_os_str() == OsStr::new("-") {
        let mut bytes = Vec::new();
        std::io::stdin()
            .lock()
            .read_to_end(&mut bytes)
            .context("Failed to read apply plan from stdin")?;
        Ok(bytes)
    } else {
        std::fs::read(path)
            .with_context(|| format!("Failed to read apply plan '{}'", path.display()))
    }
}

fn resolve_issue_ref(value: &str, refs: &BTreeMap<String, String>) -> Result<String> {
    if let Some(ref_name) = local_ref_name(value) {
        refs.get(ref_name)
            .cloned()
            .ok_or_else(|| anyhow!("Unknown local ref '${}'", ref_name))
    } else {
        Ok(value.to_string())
    }
}

fn local_ref_name(value: &str) -> Option<&str> {
    value.strip_prefix('$').filter(|name| !name.is_empty())
}

fn validate_reference_order(plan: &ApplyPlan) -> std::result::Result<(), PreflightFailure> {
    let mut defined_refs = BTreeSet::new();
    let mut duplicate_refs = BTreeSet::new();
    for operation in &plan.operations {
        if let Some(ref_name) = operation.defined_ref()
            && !defined_refs.insert(ref_name.to_string())
        {
            duplicate_refs.insert(ref_name.to_string());
        }
    }

    if let Some(duplicate) = duplicate_refs.into_iter().next() {
        let index = plan
            .operations
            .iter()
            .position(|op| op.defined_ref() == Some(duplicate.as_str()))
            .unwrap_or(0);
        return Err(PreflightFailure {
            index,
            op: plan.operations[index].op_name().to_string(),
            message: format!("Duplicate local ref '${}'", duplicate),
        });
    }

    let mut seen_refs = BTreeSet::new();
    for (index, operation) in plan.operations.iter().enumerate() {
        for value in operation.referenced_values() {
            if let Some(ref_name) = local_ref_name(value)
                && !seen_refs.contains(ref_name)
            {
                return Err(PreflightFailure {
                    index,
                    op: operation.op_name().to_string(),
                    message: format!("Operation references '${}' before it is defined", ref_name),
                });
            }
        }
        if let Some(ref_name) = operation.defined_ref() {
            seen_refs.insert(ref_name.to_string());
        }
    }

    Ok(())
}

fn load_resume_state(
    resume_path: Option<&Path>,
    checksum: &str,
) -> Result<(ApplyResumeState, bool)> {
    let Some(path) = resume_path else {
        return Ok((new_resume_state(checksum), false));
    };

    if !path.exists() {
        return Ok((new_resume_state(checksum), false));
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read resume state '{}'", path.display()))?;
    let state: ApplyResumeState = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse resume state '{}'", path.display()))?;
    if state.plan_checksum != checksum {
        bail!("Resume state checksum does not match apply plan");
    }
    Ok((state, true))
}

fn new_resume_state(checksum: &str) -> ApplyResumeState {
    ApplyResumeState {
        version: 1,
        plan_checksum: checksum.to_string(),
        completed: BTreeSet::new(),
        refs: BTreeMap::new(),
        results: Vec::new(),
    }
}

fn write_resume_state(
    resume_path: Option<&Path>,
    checksum: &str,
    completed: &BTreeSet<usize>,
    refs: &BTreeMap<String, String>,
    state_results: &BTreeMap<usize, ApplyOperationResult>,
) -> Result<()> {
    let Some(path) = resume_path else {
        return Ok(());
    };

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create resume state directory '{}'",
                parent.display()
            )
        })?;
    }

    let state = ApplyResumeState {
        version: 1,
        plan_checksum: checksum.to_string(),
        completed: completed.clone(),
        refs: refs.clone(),
        results: state_results.values().cloned().collect(),
    };
    let json = serde_json::to_vec_pretty(&state).context("Failed to serialize resume state")?;
    std::fs::write(path, json)
        .with_context(|| format!("Failed to write resume state '{}'", path.display()))?;
    Ok(())
}

fn build_output(
    success: bool,
    dry_run: bool,
    resumed: bool,
    total: usize,
    refs: BTreeMap<String, String>,
    results: Vec<ApplyOperationResult>,
) -> ApplyOutput {
    ApplyOutput {
        success,
        dry_run,
        resumed,
        summary: summarize_results(total, &results),
        refs,
        results,
    }
}

fn summarize_results(total: usize, results: &[ApplyOperationResult]) -> ApplySummary {
    let mut by_status = BTreeMap::new();
    let mut by_op = BTreeMap::new();
    for result in results {
        *by_status.entry(result.status.clone()).or_insert(0) += 1;
        *by_op.entry(result.op.clone()).or_insert(0) += 1;
    }
    ApplySummary {
        total,
        by_status,
        by_op,
    }
}

fn failure_output(
    plan: &ApplyPlan,
    dry_run: bool,
    resumed: bool,
    refs: BTreeMap<String, String>,
    failure: PreflightFailure,
) -> ApplyOutput {
    build_output(
        false,
        dry_run,
        resumed,
        plan.operations.len(),
        refs,
        vec![ApplyOperationResult::failed(
            failure.index,
            &failure.op,
            failure.message,
        )],
    )
}

fn output_apply_result(output: &ApplyOutput, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => output_json(output),
        OutputFormat::Text => {
            output_apply_text(output);
            Ok(())
        }
    }
}

fn output_apply_text(output: &ApplyOutput) {
    let status = if output.success {
        "Apply completed"
    } else {
        "Apply failed"
    };
    let dry_run = if output.dry_run { " (dry run)" } else { "" };
    let resumed = if output.resumed { " resumed" } else { "" };
    println!("{status}{dry_run}{resumed}");
    println!("{} operation(s)", output.summary.total);

    for result in &output.results {
        let issue = result.issue.as_deref().unwrap_or("-");
        let ref_name = result
            .ref_name
            .as_deref()
            .map(|name| format!(" ${name}"))
            .unwrap_or_default();
        if let Some(error) = &result.error {
            println!(
                "[{}] {} {}{}: {}",
                result.index, result.op, result.status, ref_name, error
            );
        } else {
            println!(
                "[{}] {} {} {}{}",
                result.index, result.op, result.status, issue, ref_name
            );
        }
        for warning in &result.warnings {
            println!("  warning: {warning}");
        }
    }

    if !output.refs.is_empty() {
        println!("refs:");
        for (name, value) in &output.refs {
            println!("  ${name} = {value}");
        }
    }
}

fn first_error(output: &ApplyOutput) -> String {
    output
        .results
        .iter()
        .find_map(|result| result.error.clone())
        .unwrap_or_else(|| "apply failed".to_string())
}

fn issue_output_id(issue: &Issue) -> String {
    if issue.id_readable.is_empty() {
        issue.id.clone()
    } else {
        issue.id_readable.clone()
    }
}

fn planned_ref_value(ref_name: &str) -> String {
    format!("planned:{ref_name}")
}

fn default_link_type() -> String {
    "relates".to_string()
}

fn plan_checksum(bytes: &[u8]) -> String {
    // Stable FNV-1a checksum. This is not intended for cryptographic use; it
    // only guards explicit resume files against being paired with a different plan.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracker_core::CustomFieldUpdate;

    #[test]
    fn plan_deserializes_supported_operations() {
        let plan = parse_apply_plan(
            br#"{
                "version": 1,
                "defaults": {"project": "DEMO", "validate": true},
                "operations": [
                    {"ref": "epic", "op": "create_issue", "summary": "Parent"},
                    {"op": "update_issue", "issue": "$epic", "summary": "Updated"},
                    {"op": "comment", "issue": "$epic", "body": "hello"},
                    {"op": "link", "source": "$epic", "target": "DEMO-2", "type": "relates"},
                    {"op": "delete_issue", "issue": "DEMO-9"}
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(plan.version, 1);
        assert_eq!(plan.operations.len(), 5);
        assert_eq!(plan.defaults.project.as_deref(), Some("DEMO"));
        assert_eq!(plan.defaults.validate, Some(true));
    }

    #[test]
    fn field_object_conversion_preserves_string_and_array_values() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "Priority".to_string(),
            PlanFieldValue::Single("Major".to_string()),
        );
        fields.insert(
            "Platform".to_string(),
            PlanFieldValue::Multi(vec!["macOS".to_string(), "Linux".to_string()]),
        );

        let updates = build_custom_field_updates(&fields, &[], None, None, None, None).unwrap();

        assert!(matches!(
            &updates[0],
            CustomFieldUpdate::MultiEnum { name, values }
                if name == "Platform" && values == &vec!["macOS".to_string(), "Linux".to_string()]
        ));
        assert!(matches!(
            &updates[1],
            CustomFieldUpdate::SingleEnum { name, value }
                if name == "Priority" && value == "Major"
        ));
    }

    #[test]
    fn raw_custom_fields_conversion_supports_multi_enum_shape() {
        let raw = vec![serde_json::json!({
            "$type": "MultiEnumIssueCustomField",
            "name": "Platform",
            "value": [{"name": "macOS"}, {"name": "Linux"}]
        })];

        let updates = issue::parse_custom_fields_json(&raw).unwrap();

        assert!(matches!(
            &updates[0],
            CustomFieldUpdate::MultiEnum { name, values }
                if name == "Platform" && values == &vec!["macOS".to_string(), "Linux".to_string()]
        ));
    }

    #[test]
    fn ref_resolution_uses_existing_refs() {
        let refs = BTreeMap::from([("epic".to_string(), "DEMO-1".to_string())]);
        assert_eq!(resolve_issue_ref("$epic", &refs).unwrap(), "DEMO-1");
        assert_eq!(resolve_issue_ref("DEMO-2", &refs).unwrap(), "DEMO-2");
        assert!(resolve_issue_ref("$missing", &refs).is_err());
    }

    #[test]
    fn reference_validation_rejects_forward_refs() {
        let plan = parse_apply_plan(
            br#"{
                "version": 1,
                "operations": [
                    {"op": "comment", "issue": "$epic", "body": "too soon"},
                    {"ref": "epic", "op": "create_issue", "project": "DEMO", "summary": "Parent"}
                ]
            }"#,
        )
        .unwrap();

        let failure = validate_reference_order(&plan).unwrap_err();
        assert_eq!(failure.index, 0);
        assert!(failure.message.contains("before it is defined"));
    }

    #[test]
    fn resume_checksum_mismatch_is_rejected() {
        let dir =
            std::env::temp_dir().join(format!("track-apply-resume-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let state_path = dir.join("state.json");
        std::fs::write(
            &state_path,
            r#"{"version":1,"plan_checksum":"old","completed":[],"refs":{},"results":[]}"#,
        )
        .unwrap();

        let err = load_resume_state(Some(&state_path), "new").unwrap_err();
        assert!(err.to_string().contains("checksum"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_guard_failure_output_points_at_delete_operation() {
        let plan = parse_apply_plan(
            br#"{
                "version": 1,
                "operations": [{"op": "delete_issue", "issue": "DEMO-1"}]
            }"#,
        )
        .unwrap();

        let output = failure_output(
            &plan,
            false,
            false,
            BTreeMap::new(),
            PreflightFailure {
                index: 0,
                op: "delete_issue".to_string(),
                message: "delete_issue operations require --allow-delete".to_string(),
            },
        );

        assert!(!output.success);
        assert_eq!(output.results[0].status, "failed");
        assert_eq!(output.results[0].op, "delete_issue");
    }
}
