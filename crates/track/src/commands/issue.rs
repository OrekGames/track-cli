use crate::cache::TrackerCache;
use crate::cli::{IssueCommands, OutputFormat};
use crate::output::{output_list, output_result};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use tracker_core::{
    CreateIssue, CustomFieldUpdate, Issue, IssueTracker, ProjectCustomField, UpdateIssue,
};

pub fn handle_issue(
    client: &dyn IssueTracker,
    action: &IssueCommands,
    format: OutputFormat,
    default_project: Option<&str>,
) -> Result<()> {
    match action {
        IssueCommands::Get { id, full } => handle_get(client, id, *full, format),
        IssueCommands::Create {
            project,
            summary,
            description,
            fields,
            state,
            priority,
            assignee,
            tags,
            parent,
            validate,
            dry_run,
            json,
        } => handle_create(
            client,
            project.as_deref(),
            summary.as_deref(),
            description.as_deref(),
            fields,
            state.as_deref(),
            priority.as_deref(),
            assignee.as_deref(),
            tags,
            parent.as_deref(),
            *validate,
            *dry_run,
            json.as_deref(),
            format,
            default_project,
        ),
        IssueCommands::Update {
            ids,
            summary,
            description,
            fields,
            state,
            priority,
            assignee,
            tags,
            validate,
            dry_run,
            json,
        } => handle_update_batch(
            client,
            ids,
            summary.as_deref(),
            description.as_deref(),
            fields,
            state.as_deref(),
            priority.as_deref(),
            assignee.as_deref(),
            tags,
            *validate,
            *dry_run,
            json.as_deref(),
            format,
        ),
        IssueCommands::Search {
            query,
            template,
            project,
            limit,
            skip,
        } => handle_search(
            client,
            query.as_deref(),
            template.as_deref(),
            project.as_deref(),
            *limit,
            *skip,
            format,
            default_project,
        ),
        IssueCommands::Delete { ids } => handle_delete_batch(client, ids, format),
        IssueCommands::Comment { id, text } => handle_comment(client, id, text, format),
        IssueCommands::Comments { id, limit } => handle_comments(client, id, *limit, format),
        IssueCommands::Link {
            source,
            target,
            link_type,
        } => handle_link(client, source, target, link_type, format),
        IssueCommands::Start { ids, field, state } => {
            handle_state_transition_batch(client, ids, field, state, "started", format)
        }
        IssueCommands::Complete { ids, field, state } => {
            handle_state_transition_batch(client, ids, field, state, "completed", format)
        }
    }
}

/// Record issue access in the cache for LRU tracking
fn record_issue_access(issue: &Issue) {
    // Try to load, update, and save cache; ignore errors since this is optional
    if let Ok(mut cache) = TrackerCache::load(None) {
        cache.record_issue_access(issue);
        let _ = cache.save(None);
    }
}

fn handle_get(client: &dyn IssueTracker, id: &str, full: bool, format: OutputFormat) -> Result<()> {
    let issue = client
        .get_issue(id)
        .with_context(|| format!("Failed to fetch issue '{}'", id))?;

    // Record access for LRU tracking
    record_issue_access(&issue);

    if !full {
        output_result(&issue, format);
        return Ok(());
    }

    // Fetch additional context for full view
    let links = client
        .get_issue_links(id)
        .with_context(|| format!("Failed to fetch links for '{}'", id))?;

    let comments = client
        .get_comments(id)
        .with_context(|| format!("Failed to fetch comments for '{}'", id))?;

    match format {
        OutputFormat::Json => {
            // Build a comprehensive JSON structure
            let full_issue = serde_json::json!({
                "issue": issue,
                "links": links,
                "comments": comments
            });
            println!("{}", serde_json::to_string_pretty(&full_issue)?);
        }
        OutputFormat::Text => {
            use colored::Colorize;
            // Print issue details
            output_result(&issue, format);

            // Print links
            if !links.is_empty() {
                println!("\n  {}:", "Links".dimmed());
                for link in &links {
                    let direction = link.direction.as_deref().unwrap_or("BOTH");
                    let description = match direction {
                        "INWARD" => link
                            .link_type
                            .target_to_source
                            .as_deref()
                            .unwrap_or(&link.link_type.name),
                        "OUTWARD" => link
                            .link_type
                            .source_to_target
                            .as_deref()
                            .unwrap_or(&link.link_type.name),
                        _ => &link.link_type.name,
                    };
                    for linked_issue in &link.issues {
                        let linked_id = linked_issue
                            .id_readable
                            .as_deref()
                            .unwrap_or(&linked_issue.id);
                        let linked_summary = linked_issue.summary.as_deref().unwrap_or("");
                        println!(
                            "    {} {} - {}",
                            description.dimmed(),
                            linked_id.cyan(),
                            linked_summary
                        );
                    }
                }
            }

            // Print comments
            if !comments.is_empty() {
                let recent_comments: Vec<_> = comments.iter().rev().take(5).collect();
                println!(
                    "\n  {} ({} total):",
                    "Recent Comments".dimmed(),
                    comments.len()
                );
                for comment in recent_comments.iter().rev() {
                    let author = comment
                        .author
                        .as_ref()
                        .map(|a| a.login.as_str())
                        .unwrap_or("unknown");
                    let date = comment
                        .created
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default();
                    println!("\n    [{}] {} wrote:", date.dimmed(), author.cyan());
                    for line in comment.text.lines().take(3) {
                        println!("      {}", line);
                    }
                    if comment.text.lines().count() > 3 {
                        println!("      ...");
                    }
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_create(
    client: &dyn IssueTracker,
    project: Option<&str>,
    summary: Option<&str>,
    description: Option<&str>,
    fields: &[String],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    tags: &[String],
    parent: Option<&str>,
    validate: bool,
    dry_run: bool,
    json: Option<&str>,
    format: OutputFormat,
    default_project: Option<&str>,
) -> Result<()> {
    let (create, project_id) = if let Some(payload) = json {
        let create = parse_create_payload(client, payload)?;
        let pid = create.project_id.clone();
        (create, pid)
    } else {
        // Try CLI flag first, then fall back to config default_project
        let project_input = project
            .or(default_project)
            .ok_or_else(|| {
                anyhow!(
                    "Project is required. Use -p/--project or set a default with 'track config project <ID>'"
                )
            })?
            .to_string();
        let summary = summary.ok_or_else(|| anyhow!("Summary is required"))?;

        // Resolve project shortName to internal ID
        let project_id = client
            .resolve_project_id(&project_input)
            .with_context(|| format!("Failed to resolve project '{}'", project_input))?;

        // Fetch project schema for field type detection
        let schema = if !fields.is_empty() {
            client
                .get_project_custom_fields(&project_id)
                .ok()
        } else {
            None
        };

        let custom_fields =
            build_custom_fields(fields, state, priority, assignee, schema.as_deref())?;

        let create = CreateIssue {
            project_id: project_id.clone(),
            summary: summary.to_string(),
            description: description.map(|s| s.to_string()),
            custom_fields,
            tags: tags.to_vec(),
        };

        (create, project_id)
    };

    // Validate custom fields if requested
    if validate {
        validate_custom_fields(client, &project_id, &create.custom_fields)?;

        if dry_run {
            match format {
                OutputFormat::Json => {
                    println!(
                        r#"{{"valid": true, "message": "Validation passed", "fields_validated": {}}}"#,
                        create.custom_fields.len()
                    );
                }
                OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{}", "Validation passed".green().bold());
                    println!(
                        "  {} custom fields validated against project schema",
                        create.custom_fields.len()
                    );
                }
            }
            return Ok(());
        }
    }

    let issue = client
        .create_issue(&create)
        .context("Failed to create issue")?;

    // If parent is specified, create the subtask link
    if let Some(parent_id) = parent {
        client
            .link_subtask(&issue.id_readable, parent_id)
            .with_context(|| format!("Failed to link issue as subtask of '{}'", parent_id))?;

        match format {
            OutputFormat::Json => {
                // Output JSON with parent info included
                println!(
                    r#"{{"id":"{}","idReadable":"{}","summary":"{}","parent":"{}","message":"Issue created as subtask"}}"#,
                    issue.id, issue.id_readable, issue.summary, parent_id
                );
            }
            OutputFormat::Text => {
                use colored::Colorize;
                println!(
                    "Created {} as subtask of {}: {}",
                    issue.id_readable.cyan().bold(),
                    parent_id.cyan(),
                    issue.summary
                );
            }
        }
    } else {
        output_result(&issue, format);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_update(
    client: &dyn IssueTracker,
    id: &str,
    summary: Option<&str>,
    description: Option<&str>,
    fields: &[String],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    tags: &[String],
    validate: bool,
    dry_run: bool,
    json: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let update = if let Some(payload) = json {
        parse_update_payload(payload)?
    } else {
        // Fetch project schema for field type detection when generic fields are provided
        let schema = if !fields.is_empty() {
            client
                .get_issue(id)
                .ok()
                .and_then(|issue| {
                    client
                        .get_project_custom_fields(&issue.project.id)
                        .ok()
                })
        } else {
            None
        };

        let custom_fields =
            build_custom_fields(fields, state, priority, assignee, schema.as_deref())?;

        UpdateIssue {
            summary: summary.map(|s| s.to_string()),
            description: description.map(|s| s.to_string()),
            custom_fields,
            tags: tags.to_vec(),
        }
    };

    // Validate custom fields if requested
    if validate && !update.custom_fields.is_empty() {
        // Get the issue to determine its project
        let existing_issue = client
            .get_issue(id)
            .with_context(|| format!("Failed to fetch issue '{}' for validation", id))?;

        validate_custom_fields(client, &existing_issue.project.id, &update.custom_fields)?;

        if dry_run {
            match format {
                OutputFormat::Json => {
                    println!(
                        r#"{{"valid": true, "message": "Validation passed", "issue": "{}", "fields_validated": {}}}"#,
                        id,
                        update.custom_fields.len()
                    );
                }
                OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{}", "Validation passed".green().bold());
                    println!(
                        "  {} custom fields validated for issue {}",
                        update.custom_fields.len(),
                        id.cyan()
                    );
                }
            }
            return Ok(());
        }
    }

    let issue = client
        .update_issue(id, &update)
        .with_context(|| format!("Failed to update issue '{}'", id))?;

    output_result(&issue, format);
    Ok(())
}

/// Build a list of custom field updates from CLI arguments.
///
/// When `project_fields` is provided, the field type is detected from the project schema
/// so that state fields, user fields, and enum fields get the correct `$type` discriminator.
/// Without schema info, generic `--field` arguments default to SingleEnum.
fn build_custom_fields(
    fields: &[String],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    project_fields: Option<&[ProjectCustomField]>,
) -> Result<Vec<CustomFieldUpdate>> {
    let mut custom_fields = Vec::new();

    // Parse generic field=value pairs with type detection
    for field in fields {
        let (name, value) = parse_field_value(field)?;

        let detected_type = project_fields.and_then(|pf| {
            pf.iter()
                .find(|f| f.name.eq_ignore_ascii_case(&name))
                .map(|f| f.field_type.as_str())
        });

        let update = match detected_type {
            Some(ft) if ft.contains("state") => CustomFieldUpdate::State { name, value },
            Some(ft) if ft.contains("user") => CustomFieldUpdate::SingleUser {
                name,
                login: value,
            },
            _ => CustomFieldUpdate::SingleEnum { name, value },
        };

        custom_fields.push(update);
    }

    // Add state if provided
    if let Some(state_value) = state {
        custom_fields.push(CustomFieldUpdate::State {
            name: "State".to_string(),
            value: state_value.to_string(),
        });
    }

    // Add priority if provided
    if let Some(priority_value) = priority {
        custom_fields.push(CustomFieldUpdate::SingleEnum {
            name: "Priority".to_string(),
            value: priority_value.to_string(),
        });
    }

    // Add assignee if provided
    if let Some(assignee_login) = assignee {
        custom_fields.push(CustomFieldUpdate::SingleUser {
            name: "Assignee".to_string(),
            login: assignee_login.to_string(),
        });
    }

    Ok(custom_fields)
}

/// Validate custom fields against project schema
fn validate_custom_fields(
    client: &dyn IssueTracker,
    project_id: &str,
    custom_fields: &[CustomFieldUpdate],
) -> Result<()> {
    if custom_fields.is_empty() {
        return Ok(());
    }

    // Fetch project fields schema
    let project_fields = client
        .get_project_custom_fields(project_id)
        .with_context(|| format!("Failed to fetch custom fields for project '{}'", project_id))?;

    for field in custom_fields {
        let (field_name, field_value) = match field {
            CustomFieldUpdate::SingleEnum { name, value } => (name.as_str(), value.as_str()),
            CustomFieldUpdate::State { name, value } => (name.as_str(), value.as_str()),
            CustomFieldUpdate::SingleUser { name, login } => (name.as_str(), login.as_str()),
        };

        // Find the field definition
        let field_def = project_fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(field_name));

        match field_def {
            Some(def) => {
                // If field has enum values, validate the value is in the list
                if !def.values.is_empty() {
                    let value_valid = def
                        .values
                        .iter()
                        .any(|v| v.eq_ignore_ascii_case(field_value));
                    if !value_valid {
                        return Err(anyhow!(
                            "Invalid value '{}' for field '{}'. Valid values: {}",
                            field_value,
                            field_name,
                            def.values.join(", ")
                        ));
                    }
                }
            }
            None => {
                // Field not found - this might be okay for some backends, but warn
                // We don't error here because some backends (like Jira) may have
                // fields that aren't returned by get_project_custom_fields
            }
        }
    }

    Ok(())
}

/// Parse a "Name=Value" string into (name, value) tuple
fn parse_field_value(input: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = input.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "Invalid field format '{}'. Expected 'FIELD=VALUE'",
            input
        ));
    }
    let name = parts[0].trim();
    let value = parts[1].trim();
    if name.is_empty() || value.is_empty() {
        return Err(anyhow!(
            "Invalid field format '{}'. Name and value cannot be empty",
            input
        ));
    }
    Ok((name.to_string(), value.to_string()))
}

#[derive(Deserialize)]
struct CreateIssuePayload {
    project: ProjectInput,
    summary: String,
    description: Option<String>,
    #[serde(default, rename = "customFields")]
    custom_fields: Vec<serde_json::Value>,
    #[serde(default)]
    tags: Vec<TagInput>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ProjectInput {
    Id { id: String },
    Value(String),
}

#[derive(Deserialize)]
struct TagInput {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct UpdateIssuePayload {
    summary: Option<String>,
    description: Option<String>,
    #[serde(default, rename = "customFields")]
    custom_fields: Vec<serde_json::Value>,
    #[serde(default)]
    tags: Vec<TagInput>,
}

fn parse_create_payload(client: &dyn IssueTracker, payload: &str) -> Result<CreateIssue> {
    let parsed: CreateIssuePayload =
        serde_json::from_str(payload).context("Invalid JSON payload for issue create")?;

    let project_input = match parsed.project {
        ProjectInput::Id { id } => id,
        ProjectInput::Value(id) => id,
    };

    // Resolve project ID
    let project_id = client
        .resolve_project_id(&project_input)
        .with_context(|| format!("Failed to resolve project '{}'", project_input))?;

    // Parse custom fields from raw JSON
    let custom_fields = parse_custom_fields_json(&parsed.custom_fields)?;

    // Parse tags
    let tags: Vec<String> = parsed.tags.into_iter().filter_map(|t| t.name).collect();

    Ok(CreateIssue {
        project_id,
        summary: parsed.summary,
        description: parsed.description,
        custom_fields,
        tags,
    })
}

fn parse_update_payload(payload: &str) -> Result<UpdateIssue> {
    let parsed: UpdateIssuePayload =
        serde_json::from_str(payload).context("Invalid JSON payload for issue update")?;

    // Parse custom fields from raw JSON
    let custom_fields = parse_custom_fields_json(&parsed.custom_fields)?;

    // Parse tags
    let tags: Vec<String> = parsed.tags.into_iter().filter_map(|t| t.name).collect();

    // Require at least one field to be updated
    if parsed.summary.is_none()
        && parsed.description.is_none()
        && custom_fields.is_empty()
        && tags.is_empty()
    {
        return Err(anyhow!(
            "Issue update JSON payload must include at least one field to update"
        ));
    }

    Ok(UpdateIssue {
        summary: parsed.summary,
        description: parsed.description,
        custom_fields,
        tags,
    })
}

/// Parse custom fields from raw JSON values
fn parse_custom_fields_json(fields: &[serde_json::Value]) -> Result<Vec<CustomFieldUpdate>> {
    let mut result = Vec::new();

    for field in fields {
        let field_type = field
            .get("$type")
            .and_then(|v| v.as_str())
            .unwrap_or("SingleEnumIssueCustomField");

        let name = field
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Custom field missing 'name'"))?
            .to_string();

        let custom_field = match field_type {
            "StateIssueCustomField" => {
                let value = field
                    .get("value")
                    .and_then(|v| v.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                CustomFieldUpdate::State { name, value }
            }
            "SingleUserIssueCustomField" => {
                let login = field
                    .get("value")
                    .and_then(|v| v.get("login"))
                    .and_then(|l| l.as_str())
                    .unwrap_or("")
                    .to_string();
                CustomFieldUpdate::SingleUser { name, login }
            }
            _ => {
                // Default to SingleEnum for unknown types
                let value = field
                    .get("value")
                    .and_then(|v| v.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                CustomFieldUpdate::SingleEnum { name, value }
            }
        };

        result.push(custom_field);
    }

    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn handle_search(
    client: &dyn IssueTracker,
    query: Option<&str>,
    template: Option<&str>,
    project: Option<&str>,
    limit: usize,
    skip: usize,
    format: OutputFormat,
    default_project: Option<&str>,
) -> Result<()> {
    // Resolve query from template if needed
    let actual_query = resolve_search_query(query, template, project, default_project)?;

    let issues = client
        .search_issues(&actual_query, limit, skip)
        .context("Failed to search issues")?;

    // Record first result for quick access tracking (if any)
    if let Some(first) = issues.first() {
        record_issue_access(first);
    }

    output_list(&issues, format);
    Ok(())
}

/// Resolve search query from either direct query or template
fn resolve_search_query(
    query: Option<&str>,
    template: Option<&str>,
    project: Option<&str>,
    default_project: Option<&str>,
) -> Result<String> {
    match (query, template) {
        // Direct query provided
        (Some(q), _) => Ok(q.to_string()),

        // Template provided - resolve from cache
        (None, Some(tmpl)) => {
            let cache = TrackerCache::load(None).context(
                "Cache not found. Run 'track cache refresh' first to use query templates.",
            )?;

            // Find the template
            let template_def = cache
                .query_templates
                .iter()
                .find(|qt| qt.name.eq_ignore_ascii_case(tmpl))
                .ok_or_else(|| {
                    let available: Vec<&str> = cache
                        .query_templates
                        .iter()
                        .map(|t| t.name.as_str())
                        .collect();
                    anyhow!(
                        "Template '{}' not found. Available templates: {}",
                        tmpl,
                        if available.is_empty() {
                            "(none - run 'track cache refresh')".to_string()
                        } else {
                            available.join(", ")
                        }
                    )
                })?;

            // Get project for substitution
            let proj = project.or(default_project).ok_or_else(|| {
                anyhow!(
                    "Project required for template '{}'. Use --project or set default with 'track config project <ID>'",
                    tmpl
                )
            })?;

            // Substitute {PROJECT} placeholder
            Ok(template_def.query.replace("{PROJECT}", proj))
        }

        // Neither provided
        (None, None) => Err(anyhow!("Either a search query or --template is required")),
    }
}

fn handle_delete(client: &dyn IssueTracker, id: &str, format: OutputFormat) -> Result<()> {
    client
        .delete_issue(id)
        .with_context(|| format!("Failed to delete issue '{}'", id))?;

    match format {
        OutputFormat::Json => {
            println!(r#"{{"success": true, "message": "Issue deleted"}}"#);
        }
        OutputFormat::Text => {
            use colored::Colorize;
            println!("Issue {} deleted successfully", id.cyan().bold());
        }
    }
    Ok(())
}

fn handle_comment(
    client: &dyn IssueTracker,
    id: &str,
    text: &str,
    format: OutputFormat,
) -> Result<()> {
    let comment = client
        .add_comment(id, text)
        .with_context(|| format!("Failed to add comment to issue '{}'", id))?;

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&comment)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            use colored::Colorize;
            println!("Comment added to {}:", id.cyan().bold());
            println!("  {}", text);
        }
    }
    Ok(())
}

fn handle_comments(
    client: &dyn IssueTracker,
    id: &str,
    limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let comments = client
        .get_comments(id)
        .with_context(|| format!("Failed to get comments for issue '{}'", id))?;

    let comments: Vec<_> = comments.into_iter().take(limit).collect();

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&comments)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            use colored::Colorize;
            if comments.is_empty() {
                println!("No comments on {}", id.cyan().bold());
            } else {
                println!("Comments on {} ({}):", id.cyan().bold(), comments.len());
                for comment in &comments {
                    let author = comment
                        .author
                        .as_ref()
                        .map(|a| a.login.as_str())
                        .unwrap_or("unknown");
                    let date = comment
                        .created
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default();
                    println!("\n  [{}] {} wrote:", date.dimmed(), author.cyan());
                    for line in comment.text.lines() {
                        println!("    {}", line);
                    }
                }
            }
        }
    }
    Ok(())
}

fn handle_link(
    client: &dyn IssueTracker,
    source: &str,
    target: &str,
    link_type: &str,
    format: OutputFormat,
) -> Result<()> {
    // Map user-friendly link type to backend link type name and direction
    let (backend_link_type, direction, description) = match link_type.to_lowercase().as_str() {
        "relates" => ("Relates", "BOTH", "relates to"),
        "depends" => ("Depend", "OUTWARD", "depends on"),
        "required" | "required-for" => ("Depend", "INWARD", "is required for"),
        "duplicates" | "duplicate" => ("Duplicate", "OUTWARD", "duplicates"),
        "duplicated-by" => ("Duplicate", "INWARD", "is duplicated by"),
        "subtask" | "subtask-of" => ("Subtask", "INWARD", "is subtask of"),
        "parent" | "parent-of" => ("Subtask", "OUTWARD", "is parent of"),
        _ => {
            return Err(anyhow!(
                "Unknown link type '{}'. Valid types: relates, depends, required, duplicates, duplicated-by, subtask, parent",
                link_type
            ));
        }
    };

    client
        .link_issues(source, target, backend_link_type, direction)
        .with_context(|| format!("Failed to link {} to {}", source, target))?;

    match format {
        OutputFormat::Json => {
            println!(
                r#"{{"success":true,"source":"{}","target":"{}","linkType":"{}","description":"{}"}}"#,
                source, target, link_type, description
            );
        }
        OutputFormat::Text => {
            use colored::Colorize;
            println!(
                "{} {} {}",
                source.cyan().bold(),
                description.dimmed(),
                target.cyan().bold()
            );
        }
    }
    Ok(())
}

fn handle_state_transition(
    client: &dyn IssueTracker,
    id: &str,
    field: &str,
    state: &str,
    action: &str,
    format: OutputFormat,
) -> Result<()> {
    // Build the custom field update based on field name
    let custom_field = if field.eq_ignore_ascii_case("State") || field.eq_ignore_ascii_case("Stage")
    {
        CustomFieldUpdate::State {
            name: field.to_string(),
            value: state.to_string(),
        }
    } else {
        // For non-State fields, treat as SingleEnum
        CustomFieldUpdate::SingleEnum {
            name: field.to_string(),
            value: state.to_string(),
        }
    };

    let update = UpdateIssue {
        summary: None,
        description: None,
        custom_fields: vec![custom_field],
        tags: vec![],
    };

    let issue = client
        .update_issue(id, &update)
        .with_context(|| format!("Failed to update issue '{}' state to '{}'", id, state))?;

    match format {
        OutputFormat::Json => {
            println!(
                r#"{{"success":true,"id":"{}","idReadable":"{}","action":"{}","state":"{}"}}"#,
                issue.id, issue.id_readable, action, state
            );
        }
        OutputFormat::Text => {
            use colored::Colorize;
            println!(
                "{} {} ({}={})",
                issue.id_readable.cyan().bold(),
                action.green(),
                field.dimmed(),
                state.green()
            );
        }
    }
    Ok(())
}

// ============================================================================
// Batch Operations
// ============================================================================

/// Result of a batch operation on a single issue
#[derive(Clone, serde::Serialize)]
struct BatchResult {
    id: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id_readable: Option<String>,
}

/// Summary of a batch operation
#[derive(serde::Serialize)]
struct BatchSummary {
    total: usize,
    succeeded: usize,
    failed: usize,
    results: Vec<BatchResult>,
}

#[allow(clippy::too_many_arguments)]
fn handle_update_batch(
    client: &dyn IssueTracker,
    ids: &[String],
    summary: Option<&str>,
    description: Option<&str>,
    fields: &[String],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    tags: &[String],
    validate: bool,
    dry_run: bool,
    json: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    // Single issue - delegate to original handler
    if ids.len() == 1 {
        return handle_update(
            client,
            &ids[0],
            summary,
            description,
            fields,
            state,
            priority,
            assignee,
            tags,
            validate,
            dry_run,
            json,
            format,
        );
    }

    // Batch update
    let mut results = Vec::new();

    for id in ids {
        let result = handle_update_single(
            client,
            id,
            summary,
            description,
            fields,
            state,
            priority,
            assignee,
            tags,
            validate,
            dry_run,
            json,
        );

        match result {
            Ok(issue) => {
                results.push(BatchResult {
                    id: id.clone(),
                    success: true,
                    error: None,
                    id_readable: Some(issue.id_readable),
                });
            }
            Err(e) => {
                results.push(BatchResult {
                    id: id.clone(),
                    success: false,
                    error: Some(e.to_string()),
                    id_readable: None,
                });
            }
        }
    }

    output_batch_results(&results, "updated", format);
    Ok(())
}

/// Internal update function that returns the issue instead of printing
#[allow(clippy::too_many_arguments)]
fn handle_update_single(
    client: &dyn IssueTracker,
    id: &str,
    summary: Option<&str>,
    description: Option<&str>,
    fields: &[String],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
    tags: &[String],
    validate: bool,
    _dry_run: bool,
    json: Option<&str>,
) -> Result<Issue> {
    let update = if let Some(payload) = json {
        parse_update_payload(payload)?
    } else {
        // Fetch project schema for field type detection when generic fields are provided
        let schema = if !fields.is_empty() {
            client
                .get_issue(id)
                .ok()
                .and_then(|issue| {
                    client
                        .get_project_custom_fields(&issue.project.id)
                        .ok()
                })
        } else {
            None
        };

        let custom_fields =
            build_custom_fields(fields, state, priority, assignee, schema.as_deref())?;

        UpdateIssue {
            summary: summary.map(|s| s.to_string()),
            description: description.map(|s| s.to_string()),
            custom_fields,
            tags: tags.to_vec(),
        }
    };

    // Validate custom fields if requested
    if validate && !update.custom_fields.is_empty() {
        let existing_issue = client
            .get_issue(id)
            .with_context(|| format!("Failed to fetch issue '{}' for validation", id))?;

        validate_custom_fields(client, &existing_issue.project.id, &update.custom_fields)?;
    }

    let issue = client
        .update_issue(id, &update)
        .with_context(|| format!("Failed to update issue '{}'", id))?;

    Ok(issue)
}

fn handle_delete_batch(
    client: &dyn IssueTracker,
    ids: &[String],
    format: OutputFormat,
) -> Result<()> {
    // Single issue - delegate to original handler
    if ids.len() == 1 {
        return handle_delete(client, &ids[0], format);
    }

    // Batch delete
    let mut results = Vec::new();

    for id in ids {
        let result = client.delete_issue(id);

        match result {
            Ok(()) => {
                results.push(BatchResult {
                    id: id.clone(),
                    success: true,
                    error: None,
                    id_readable: Some(id.clone()),
                });
            }
            Err(e) => {
                results.push(BatchResult {
                    id: id.clone(),
                    success: false,
                    error: Some(e.to_string()),
                    id_readable: None,
                });
            }
        }
    }

    output_batch_results(&results, "deleted", format);
    Ok(())
}

fn handle_state_transition_batch(
    client: &dyn IssueTracker,
    ids: &[String],
    field: &str,
    state: &str,
    action: &str,
    format: OutputFormat,
) -> Result<()> {
    // Single issue - delegate to original handler
    if ids.len() == 1 {
        return handle_state_transition(client, &ids[0], field, state, action, format);
    }

    // Build the custom field update
    let custom_field = if field.eq_ignore_ascii_case("State") || field.eq_ignore_ascii_case("Stage")
    {
        CustomFieldUpdate::State {
            name: field.to_string(),
            value: state.to_string(),
        }
    } else {
        CustomFieldUpdate::SingleEnum {
            name: field.to_string(),
            value: state.to_string(),
        }
    };

    let update = UpdateIssue {
        summary: None,
        description: None,
        custom_fields: vec![custom_field],
        tags: vec![],
    };

    // Batch state transition
    let mut results = Vec::new();

    for id in ids {
        let result = client.update_issue(id, &update);

        match result {
            Ok(issue) => {
                results.push(BatchResult {
                    id: id.clone(),
                    success: true,
                    error: None,
                    id_readable: Some(issue.id_readable),
                });
            }
            Err(e) => {
                results.push(BatchResult {
                    id: id.clone(),
                    success: false,
                    error: Some(e.to_string()),
                    id_readable: None,
                });
            }
        }
    }

    output_batch_results(&results, action, format);
    Ok(())
}

/// Output batch operation results in the appropriate format
fn output_batch_results(results: &[BatchResult], action: &str, format: OutputFormat) {
    let succeeded = results.iter().filter(|r| r.success).count();
    let failed = results.len() - succeeded;

    let summary = BatchSummary {
        total: results.len(),
        succeeded,
        failed,
        results: results.to_vec(),
    };

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        }
        OutputFormat::Text => {
            use colored::Colorize;

            // Print summary line
            if failed == 0 {
                println!(
                    "{} {} issues {}",
                    "✓".green().bold(),
                    succeeded,
                    action.green()
                );
            } else {
                println!(
                    "{} {}/{} issues {} ({} failed)",
                    if succeeded > 0 {
                        "⚠".yellow()
                    } else {
                        "✗".red()
                    },
                    succeeded,
                    results.len(),
                    action,
                    failed
                );
            }

            // Print individual results
            for result in results {
                let id = result.id_readable.as_deref().unwrap_or(&result.id);
                if result.success {
                    println!("  {} {}", "✓".green(), id.cyan());
                } else {
                    let error = result.error.as_deref().unwrap_or("Unknown error");
                    println!("  {} {} - {}", "✗".red(), id.cyan(), error.red());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_field_value_correctly() {
        let (name, value) = parse_field_value("Priority=Major").unwrap();
        assert_eq!(name, "Priority");
        assert_eq!(value, "Major");
    }

    #[test]
    fn parses_field_value_with_equals_in_value() {
        let (name, value) = parse_field_value("Formula=a=b+c").unwrap();
        assert_eq!(name, "Formula");
        assert_eq!(value, "a=b+c");
    }

    #[test]
    fn rejects_field_value_without_equals() {
        assert!(parse_field_value("InvalidFormat").is_err());
    }

    #[test]
    fn rejects_field_value_with_empty_name() {
        assert!(parse_field_value("=Value").is_err());
    }

    #[test]
    fn rejects_field_value_with_empty_value() {
        assert!(parse_field_value("Name=").is_err());
    }

    #[test]
    fn builds_custom_fields_from_cli_args() {
        let fields = vec!["Type=Bug".to_string(), "Component=UI".to_string()];
        let result =
            build_custom_fields(&fields, Some("Open"), Some("Major"), Some("john"), None).unwrap();

        assert_eq!(result.len(), 5); // 2 fields + state + priority + assignee
    }

    #[test]
    fn builds_custom_fields_with_type_detection() {
        use tracker_core::ProjectCustomField;

        let schema = vec![
            ProjectCustomField {
                id: "1".to_string(),
                name: "Phase".to_string(),
                field_type: "state[1]".to_string(),
                required: true,
                values: vec!["Planning".to_string(), "Development".to_string()],
                state_values: vec![],
            },
            ProjectCustomField {
                id: "2".to_string(),
                name: "System".to_string(),
                field_type: "enum[1]".to_string(),
                required: true,
                values: vec!["Web".to_string(), "Mobile".to_string()],
                state_values: vec![],
            },
            ProjectCustomField {
                id: "3".to_string(),
                name: "Reviewer".to_string(),
                field_type: "user[1]".to_string(),
                required: false,
                values: vec![],
                state_values: vec![],
            },
        ];

        let fields = vec![
            "Phase=Planning".to_string(),
            "System=Web".to_string(),
            "Reviewer=alice".to_string(),
        ];
        let result = build_custom_fields(&fields, None, None, None, Some(&schema)).unwrap();

        assert_eq!(result.len(), 3);
        assert!(
            matches!(&result[0], CustomFieldUpdate::State { name, value } if name == "Phase" && value == "Planning"),
            "Phase should be detected as State, got: {:?}",
            result[0]
        );
        assert!(
            matches!(&result[1], CustomFieldUpdate::SingleEnum { name, value } if name == "System" && value == "Web"),
            "System should be detected as SingleEnum, got: {:?}",
            result[1]
        );
        assert!(
            matches!(&result[2], CustomFieldUpdate::SingleUser { name, login } if name == "Reviewer" && login == "alice"),
            "Reviewer should be detected as SingleUser, got: {:?}",
            result[2]
        );
    }

    #[test]
    fn resolve_query_returns_direct_query() {
        let result = resolve_search_query(Some("project: PROJ"), None, None, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "project: PROJ");
    }

    #[test]
    fn resolve_query_requires_query_or_template() {
        let result = resolve_search_query(None, None, None, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Either a search query"));
    }
}
