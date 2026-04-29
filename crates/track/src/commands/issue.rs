use crate::cache::TrackerCache;
use crate::cli::{IssueCommands, OutputFormat};
use crate::output::{output_json, output_list, output_page_hint, output_progress, output_result};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use tracker_core::{
    CreateIssue, CustomFieldUpdate, Issue, IssueTracker, ProjectCustomField, UpdateIssue,
    fetch_all_pages,
};

/// Shared fields for create, update, and batch-update commands.
struct IssueFieldArgs<'a> {
    summary: Option<&'a str>,
    description: Option<&'a str>,
    fields: &'a [String],
    state: Option<&'a str>,
    priority: Option<&'a str>,
    assignee: Option<&'a str>,
    tags: &'a [String],
    parent: Option<&'a str>,
    validate: bool,
    dry_run: bool,
    json: Option<&'a str>,
}

/// Arguments for issue search.
struct SearchArgs<'a> {
    query: Option<&'a str>,
    template: Option<&'a str>,
    project: Option<&'a str>,
    limit: usize,
    skip: usize,
    all: bool,
}

pub fn handle_issue(
    client: &dyn IssueTracker,
    action: &IssueCommands,
    format: OutputFormat,
    default_project: Option<&str>,
    verbose: bool,
) -> Result<()> {
    match action {
        IssueCommands::Get { id, full } => handle_get(client, id, *full, format),
        IssueCommands::Create {
            project,
            summary,
            description,
            body_file,
            fields,
            state,
            priority,
            assignee,
            tags,
            parent,
            validate,
            dry_run,
            json,
        } => {
            let resolved_desc = super::resolve_body(description.as_deref(), body_file.as_deref())?;
            let args = IssueFieldArgs {
                summary: summary.as_deref(),
                description: resolved_desc.as_deref(),
                fields,
                state: state.as_deref(),
                priority: priority.as_deref(),
                assignee: assignee.as_deref(),
                tags,
                parent: None, // parent is passed separately to handle_create
                validate: *validate,
                dry_run: *dry_run,
                json: json.as_deref(),
            };
            handle_create(
                client,
                &args,
                project.as_deref(),
                parent.as_deref(),
                format,
                default_project,
                verbose,
            )
        }
        IssueCommands::Update {
            ids,
            summary,
            description,
            body_file,
            fields,
            state,
            priority,
            assignee,
            tags,
            parent,
            validate,
            dry_run,
            json,
        } => {
            let resolved_desc = super::resolve_body(description.as_deref(), body_file.as_deref())?;
            let args = IssueFieldArgs {
                summary: summary.as_deref(),
                description: resolved_desc.as_deref(),
                fields,
                state: state.as_deref(),
                priority: priority.as_deref(),
                assignee: assignee.as_deref(),
                tags,
                parent: parent.as_deref(),
                validate: *validate,
                dry_run: *dry_run,
                json: json.as_deref(),
            };
            handle_update_batch(client, ids, &args, format, verbose)
        }
        IssueCommands::Search {
            query,
            template,
            project,
            limit,
            skip,
            all,
        } => {
            let args = SearchArgs {
                query: query.as_deref(),
                template: template.as_deref(),
                project: project.as_deref(),
                limit: *limit,
                skip: *skip,
                all: *all,
            };
            handle_search(client, &args, format, default_project)
        }
        IssueCommands::Delete { ids } => handle_delete_batch(client, ids, format),
        IssueCommands::Attachments { id } => handle_attachments(client, id, format),
        IssueCommands::Attach {
            id,
            paths,
            name,
            mime_type,
            comment,
            silent,
        } => handle_attach(
            client,
            id,
            paths,
            name.as_deref(),
            mime_type.as_deref(),
            comment.as_deref(),
            *silent,
            format,
        ),
        IssueCommands::Comment {
            id,
            text,
            body_file,
            attach,
            name,
            mime_type,
            silent,
        } => {
            let resolved_text = super::resolve_body(text.as_deref(), body_file.as_deref())?
                .ok_or_else(|| anyhow!("Comment text is required"))?;
            handle_comment(
                client,
                id,
                &resolved_text,
                attach,
                name.as_deref(),
                mime_type.as_deref(),
                *silent,
                format,
            )
        }
        IssueCommands::Comments { id, limit, all } => {
            handle_comments(client, id, *limit, *all, format)
        }
        IssueCommands::Link {
            source,
            target,
            link_type,
        } => handle_link(client, source, target, link_type, format),
        IssueCommands::Unlink { source, link_id } => handle_unlink(client, source, link_id, format),
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
    // Load only the runtime shard, update, and save just that shard
    if let Ok(mut cache) = TrackerCache::load(None) {
        let _ = cache.ensure_runtime_shards();
        cache.record_issue_access(issue);
        let _ = cache.save_runtime(None);
    }
}

fn handle_get(client: &dyn IssueTracker, id: &str, full: bool, format: OutputFormat) -> Result<()> {
    let issue = client
        .get_issue(id)
        .with_context(|| format!("Failed to fetch issue '{}'", id))?;

    // Record access for LRU tracking
    record_issue_access(&issue);

    if !full {
        output_result(&issue, format)?;
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
            output_json(&full_issue)?;
        }
        OutputFormat::Text => {
            use colored::Colorize;
            // Print issue details
            output_result(&issue, format)?;

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

fn handle_create(
    client: &dyn IssueTracker,
    args: &IssueFieldArgs,
    project: Option<&str>,
    parent: Option<&str>,
    format: OutputFormat,
    default_project: Option<&str>,
    verbose: bool,
) -> Result<()> {
    let (create, project_id) = if let Some(payload) = args.json {
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
        let summary = args.summary.ok_or_else(|| anyhow!("Summary is required"))?;

        // Resolve project shortName to internal ID
        let project_id = client
            .resolve_project_id(&project_input)
            .with_context(|| format!("Failed to resolve project '{}'", project_input))?;

        // Fetch project schema for field type detection
        let schema = if !args.fields.is_empty() {
            client.get_project_custom_fields(&project_id).ok()
        } else {
            None
        };

        let custom_fields = build_custom_fields(
            args.fields,
            args.state,
            args.priority,
            args.assignee,
            schema.as_deref(),
        )?;

        let create = CreateIssue {
            project_id: project_id.clone(),
            summary: summary.to_string(),
            description: args.description.map(|s| s.to_string()),
            custom_fields,
            tags: args.tags.to_vec(),
            parent: parent.map(|s| s.to_string()),
        };

        (create, project_id)
    };

    // Validate custom fields if requested
    if args.validate {
        validate_custom_fields(client, &project_id, &create.custom_fields)?;

        if args.dry_run {
            match format {
                OutputFormat::Json => {
                    let response = serde_json::json!({
                        "valid": true,
                        "message": "Validation passed",
                        "fields_validated": create.custom_fields.len(),
                    });
                    output_json(&response)?;
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

    let warnings = verify_issue_create(&create, &issue);
    crate::output::output_verification_warnings(&warnings, format);

    if verbose {
        crate::output::output_change_summary(None, &issue, None, Some(&create), format);
        println!();
    }

    output_result(&issue, format)?;

    Ok(())
}

fn handle_update(
    client: &dyn IssueTracker,
    id: &str,
    args: &IssueFieldArgs,
    format: OutputFormat,
    verbose: bool,
) -> Result<()> {
    let update = build_update(client, id, args)?;

    if args.dry_run {
        let fields_validated = validate_update(client, id, &update)?;
        output_update_dry_run(id, fields_validated, format);
        return Ok(());
    }

    // Validate custom fields if requested
    if args.validate {
        validate_update(client, id, &update)?;
    }

    // Fetch old state if verbose for diffing
    let old_issue = if verbose {
        client.get_issue(id).ok()
    } else {
        None
    };

    let issue = client
        .update_issue(id, &update)
        .with_context(|| format!("Failed to update issue '{}'", id))?;

    let warnings = verify_issue_update(&update, &issue);
    crate::output::output_verification_warnings(&warnings, format);

    if verbose {
        crate::output::output_change_summary(
            old_issue.as_ref(),
            &issue,
            Some(&update),
            None,
            format,
        );
        println!();
    }

    output_result(&issue, format)?;
    Ok(())
}

fn build_update(client: &dyn IssueTracker, id: &str, args: &IssueFieldArgs) -> Result<UpdateIssue> {
    let update = if let Some(payload) = args.json {
        parse_update_payload(payload)?
    } else {
        // Fetch project schema for field type detection when generic fields are provided
        let schema = if !args.fields.is_empty() {
            client
                .get_issue(id)
                .ok()
                .and_then(|issue| client.get_project_custom_fields(&issue.project.id).ok())
        } else {
            None
        };

        let custom_fields = build_custom_fields(
            args.fields,
            args.state,
            args.priority,
            args.assignee,
            schema.as_deref(),
        )?;

        UpdateIssue {
            summary: args.summary.map(|s| s.to_string()),
            description: args.description.map(|s| s.to_string()),
            custom_fields,
            tags: args.tags.to_vec(),
            parent: args.parent.map(|s| s.to_string()),
        }
    };

    Ok(update)
}

fn validate_update(client: &dyn IssueTracker, id: &str, update: &UpdateIssue) -> Result<usize> {
    if !update.custom_fields.is_empty() {
        let existing_issue = client
            .get_issue(id)
            .with_context(|| format!("Failed to fetch issue '{}' for validation", id))?;

        validate_custom_fields(client, &existing_issue.project.id, &update.custom_fields)?;
    }

    Ok(update.custom_fields.len())
}

fn output_update_dry_run(id: &str, fields_validated: usize, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!(
                r#"{{"valid": true, "message": "Validation passed", "issue": "{}", "fields_validated": {}}}"#,
                id, fields_validated
            );
        }
        OutputFormat::Text => {
            use colored::Colorize;
            println!("{}", "Validation passed".green().bold());
            println!(
                "  {} custom fields validated for issue {}",
                fields_validated,
                id.cyan()
            );
        }
    }
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

        let matched_field =
            project_fields.and_then(|pf| pf.iter().find(|f| f.name.eq_ignore_ascii_case(&name)));

        if matched_field.is_none() && project_fields.is_some() {
            eprintln!(
                "Warning: field '{}' not found in project schema. \
                 The update may be silently ignored by the server. \
                 Use 'track project fields <PROJECT>' to see available fields, \
                 or use --validate to catch this as an error.",
                name
            );
        }

        let detected_type = matched_field.map(|f| f.field_type.as_str());

        let update = match detected_type {
            Some(ft) if ft.contains("state") => CustomFieldUpdate::State { name, value },
            Some(ft) if ft.contains("user") => CustomFieldUpdate::SingleUser { name, login: value },
            // enum[*] = multi-enum, supports comma-separated values
            Some(ft) if ft.contains("enum[*]") || ft.contains("multi-enum") => {
                let values = value.split(',').map(|v| v.trim().to_string()).collect();
                CustomFieldUpdate::MultiEnum { name, values }
            }
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
        let field_name = match field {
            CustomFieldUpdate::SingleEnum { name, .. } => name.as_str(),
            CustomFieldUpdate::MultiEnum { name, .. } => name.as_str(),
            CustomFieldUpdate::State { name, .. } => name.as_str(),
            CustomFieldUpdate::SingleUser { name, .. } => name.as_str(),
        };

        // Find the field definition
        let field_def = project_fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(field_name));

        match field_def {
            Some(def) => {
                if !def.values.is_empty() {
                    // Collect all values to validate
                    let values_to_check: Vec<&str> = match field {
                        CustomFieldUpdate::SingleEnum { value, .. } => vec![value.as_str()],
                        CustomFieldUpdate::MultiEnum { values, .. } => {
                            values.iter().map(|v| v.as_str()).collect()
                        }
                        CustomFieldUpdate::State { value, .. } => vec![value.as_str()],
                        CustomFieldUpdate::SingleUser { login, .. } => vec![login.as_str()],
                    };

                    for val in &values_to_check {
                        let value_valid = def.values.iter().any(|v| v.eq_ignore_ascii_case(val));
                        if !value_valid {
                            return Err(anyhow!(
                                "Invalid value '{}' for field '{}'. Valid values: {}",
                                val,
                                field_name,
                                def.values.join(", ")
                            ));
                        }
                    }
                }
            }
            None => {
                return Err(anyhow!(
                    "Unknown field '{}'. Use 'track project fields <PROJECT>' to see available fields.",
                    field_name
                ));
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
    parent: Option<String>,
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
    parent: Option<String>,
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
        parent: parsed.parent,
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
        && parsed.parent.is_none()
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
        parent: parsed.parent,
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

fn handle_search(
    client: &dyn IssueTracker,
    args: &SearchArgs,
    format: OutputFormat,
    default_project: Option<&str>,
) -> Result<()> {
    // Resolve query from template if needed
    let actual_query =
        resolve_search_query(args.query, args.template, args.project, default_project)?;

    let (issues, inline_total) = if args.all {
        // Auto-paginate using the helper; default page size 100
        let page_size = 100usize;
        let res = fetch_all_pages(
            |offset, page_limit| {
                client
                    .search_issues(&actual_query, page_limit, offset)
                    .map(|r| r.items)
            },
            page_size,
        )
        .context("Failed to search issues (pagination)")?;
        output_progress(&format!("Fetched {} issues", res.len()), format);
        (res, None)
    } else {
        let result = client
            .search_issues(&actual_query, args.limit, args.skip)
            .context("Failed to search issues")?;
        (result.items, result.total)
    };

    // Record first result for quick access tracking (if any)
    if let Some(first) = issues.first() {
        record_issue_access(first);
    }

    output_list(&issues, format)?;

    // Pagination hint — priority cascade: inline total > cached total > heuristic
    if !args.all {
        let total_info = inline_total
            .map(|t| (t, "live".to_string()))
            .or_else(|| try_cached_count(&actual_query, args.skip));
        output_page_hint(
            issues.len(),
            args.limit,
            args.skip,
            total_info.as_ref().map(|(n, s)| (*n, s.as_str())),
            format,
        );
    }

    Ok(())
}

/// Try to match a search query against cached template counts.
/// Returns (count, age_string) if the query matches a known template for a known project.
fn try_cached_count(query: &str, skip: usize) -> Option<(u64, String)> {
    // Only useful on first page — cached counts represent totals
    if skip > 0 {
        return None;
    }
    let cache = TrackerCache::load_all(None).ok()?;
    let age = cache.age_string();

    // Check if the query matches any expanded template
    for project in &cache.projects {
        for template in &cache.query_templates {
            let expanded = template.query.replace("{PROJECT}", &project.short_name);
            if expanded.eq_ignore_ascii_case(query)
                && let Some(count) = cache.get_issue_count(&project.short_name, &template.name)
            {
                return Some((count, age));
            }
        }
    }
    None
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
            let cache = TrackerCache::load_all(None)?;

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
            let response = serde_json::json!({
                "success": true,
                "message": "Issue deleted",
            });
            output_json(&response)?;
        }
        OutputFormat::Text => {
            use colored::Colorize;
            println!("Issue {} deleted successfully", id.cyan().bold());
        }
    }
    Ok(())
}

fn handle_attachments(client: &dyn IssueTracker, id: &str, format: OutputFormat) -> Result<()> {
    let attachments = client
        .list_issue_attachments(id)
        .with_context(|| format!("Failed to list attachments for issue '{}'", id))?;

    output_list(&attachments, format)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_attach(
    client: &dyn IssueTracker,
    id: &str,
    paths: &[std::path::PathBuf],
    name: Option<&str>,
    mime_type: Option<&str>,
    comment: Option<&str>,
    silent: bool,
    format: OutputFormat,
) -> Result<()> {
    let upload = super::attachments::build_attachment_upload(
        paths, name, mime_type, comment, silent, false,
    )?;

    let attachments = client
        .add_issue_attachment(id, &upload)
        .with_context(|| format!("Failed to upload attachment(s) to issue '{}'", id))?;

    output_list(&attachments, format)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_comment(
    client: &dyn IssueTracker,
    id: &str,
    text: &str,
    attach: &[std::path::PathBuf],
    name: Option<&str>,
    mime_type: Option<&str>,
    silent: bool,
    format: OutputFormat,
) -> Result<()> {
    let comment = if attach.is_empty() {
        client
            .add_comment(id, text)
            .with_context(|| format!("Failed to add comment to issue '{}'", id))?
    } else {
        if !client.supports_issue_comment_attachments() {
            return Err(anyhow!(
                "Issue comment attachment upload is not supported by this backend"
            ));
        }

        let upload = super::attachments::build_attachment_upload(
            attach, name, mime_type, None, silent, false,
        )?;

        client
            .add_issue_comment_attachment(id, text, &upload)
            .with_context(|| format!("Failed to add comment attachment(s) to issue '{}'", id))?
    };

    match format {
        OutputFormat::Json => {
            output_json(&comment)?;
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
    all: bool,
    format: OutputFormat,
) -> Result<()> {
    let comments = if all {
        fetch_all_pages(
            |offset, page_limit| client.get_comments_page(id, page_limit, offset),
            100,
        )
        .with_context(|| format!("Failed to get comments for issue '{}'", id))?
    } else {
        client
            .get_comments_page(id, limit, 0)
            .with_context(|| format!("Failed to get comments for issue '{}'", id))?
    };

    match format {
        OutputFormat::Json => {
            output_json(&comments)?;
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
    // Parent-child relationships use link_subtask() — each backend implements this natively
    match link_type.to_lowercase().as_str() {
        "subtask" | "subtask-of" => {
            client
                .link_subtask(source, target)
                .with_context(|| format!("Failed to set {} as subtask of {}", source, target))?;
            let description = "is subtask of";
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
            return Ok(());
        }
        "parent" | "parent-of" => {
            client
                .link_subtask(target, source)
                .with_context(|| format!("Failed to set {} as parent of {}", source, target))?;
            let description = "is parent of";
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
            return Ok(());
        }
        _ => {}
    }

    // All other link types use link_issues()
    let lowered = link_type.to_lowercase();
    let (canonical_type, direction, description) = match lowered.as_str() {
        "relates" => ("relates", "BOTH", "relates to"),
        "depends" => ("depends", "OUTWARD", "depends on"),
        "required" | "required-for" => ("required", "INWARD", "is required for"),
        "duplicates" | "duplicate" => ("duplicates", "OUTWARD", "duplicates"),
        "duplicated-by" => ("duplicated-by", "INWARD", "is duplicated by"),
        // Pass through unrecognized types to the backend as-is (bidirectional default).
        // This supports custom link types defined by backend admins, either directly
        // by native name or via link_mappings config.
        _ => (lowered.as_str(), "BOTH", lowered.as_str()),
    };

    client
        .link_issues(source, target, canonical_type, direction)
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

fn handle_unlink(
    client: &dyn IssueTracker,
    source: &str,
    link_id: &str,
    format: OutputFormat,
) -> Result<()> {
    client
        .unlink_issues(source, link_id)
        .with_context(|| format!("Failed to unlink {} (link {})", source, link_id))?;

    match format {
        OutputFormat::Json => {
            println!(
                r#"{{"success":true,"source":"{}","linkId":"{}"}}"#,
                source, link_id
            );
        }
        OutputFormat::Text => {
            use colored::Colorize;
            println!(
                "{} unlinked (link {} removed)",
                source.cyan().bold(),
                link_id.dimmed()
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
        parent: None,
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

fn handle_update_batch(
    client: &dyn IssueTracker,
    ids: &[String],
    args: &IssueFieldArgs,
    format: OutputFormat,
    verbose: bool,
) -> Result<()> {
    // Single issue - delegate to original handler
    if ids.len() == 1 {
        return handle_update(client, &ids[0], args, format, verbose);
    }

    // Batch update
    let mut results = Vec::new();

    for id in ids {
        let result = if args.dry_run {
            build_update(client, id, args)
                .and_then(|update| validate_update(client, id, &update))
                .map(|_| None)
        } else {
            handle_update_single(client, id, args).map(Some)
        };

        match result {
            Ok(issue) => {
                results.push(BatchResult {
                    id: id.clone(),
                    success: true,
                    error: None,
                    id_readable: issue.map(|issue| issue.id_readable),
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

    let action = if args.dry_run { "validated" } else { "updated" };
    output_batch_results(&results, action, format)?;
    Ok(())
}

/// Internal update function that returns the issue instead of printing
fn handle_update_single(
    client: &dyn IssueTracker,
    id: &str,
    args: &IssueFieldArgs,
) -> Result<Issue> {
    let update = build_update(client, id, args)?;

    if args.validate {
        validate_update(client, id, &update)?;
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

    output_batch_results(&results, "deleted", format)?;
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
        parent: None,
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

    output_batch_results(&results, action, format)?;
    Ok(())
}

/// Output batch operation results in the appropriate format
fn output_batch_results(results: &[BatchResult], action: &str, format: OutputFormat) -> Result<()> {
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
            output_json(&summary)?;
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

    Ok(())
}

/// Verify that an issue update was correctly applied by the backend.
/// Returns a list of warning messages for fields that do not match the request.
fn verify_issue_update(requested: &UpdateIssue, result: &Issue) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Some(req_summary) = &requested.summary
        && result.summary != *req_summary
    {
        warnings.push(format!(
            "Summary: expected '{}', got '{}'",
            req_summary, result.summary
        ));
    }

    if let Some(req_desc) = &requested.description
        && result.description.as_deref() != Some(req_desc)
    {
        warnings.push("Description: update was not applied correctly".to_string());
    }

    for req_field in &requested.custom_fields {
        if let Some(warning) = verify_field_match(req_field, &result.custom_fields) {
            warnings.push(warning);
        }
    }

    warnings
}

/// Verify that an issue creation correctly applied all requested fields.
fn verify_issue_create(requested: &CreateIssue, result: &Issue) -> Vec<String> {
    let mut warnings = Vec::new();

    if result.summary != requested.summary {
        warnings.push(format!(
            "Summary: expected '{}', got '{}'",
            requested.summary, result.summary
        ));
    }

    if let Some(req_desc) = &requested.description
        && result.description.as_deref() != Some(req_desc)
    {
        warnings.push("Description: was not saved correctly".to_string());
    }

    for req_field in &requested.custom_fields {
        if let Some(warning) = verify_field_match(req_field, &result.custom_fields) {
            warnings.push(warning);
        }
    }

    warnings
}

/// Helper to verify if a requested custom field update is reflected in the issue's custom fields.
fn verify_field_match(
    requested: &CustomFieldUpdate,
    actual_fields: &[tracker_core::CustomField],
) -> Option<String> {
    use tracker_core::CustomField;

    let req_name = match requested {
        CustomFieldUpdate::SingleEnum { name, .. } => name,
        CustomFieldUpdate::MultiEnum { name, .. } => name,
        CustomFieldUpdate::State { name, .. } => name,
        CustomFieldUpdate::SingleUser { name, .. } => name,
    };

    let actual = actual_fields.iter().find(|f| match f {
        CustomField::SingleEnum { name, .. } => name.eq_ignore_ascii_case(req_name),
        CustomField::State { name, .. } => name.eq_ignore_ascii_case(req_name),
        CustomField::SingleUser { name, .. } => name.eq_ignore_ascii_case(req_name),
        CustomField::Text { name, .. } => name.eq_ignore_ascii_case(req_name),
        CustomField::MultiEnum { name, .. } => name.eq_ignore_ascii_case(req_name),
        CustomField::Unknown { name } => name.eq_ignore_ascii_case(req_name),
    });

    match (requested, actual) {
        (
            CustomFieldUpdate::SingleEnum { name, value },
            Some(CustomField::SingleEnum {
                value: actual_val, ..
            }),
        ) => {
            if actual_val.as_deref().unwrap_or("") != value {
                Some(format!(
                    "Field '{}': expected '{}', got '{}'",
                    name,
                    value,
                    actual_val.as_deref().unwrap_or("None")
                ))
            } else {
                None
            }
        }
        (
            CustomFieldUpdate::MultiEnum { name, values },
            Some(CustomField::MultiEnum {
                values: actual_vals,
                ..
            }),
        ) => {
            if actual_vals != values {
                Some(format!(
                    "Field '{}': expected '{}', got '{}'",
                    name,
                    values.join(", "),
                    actual_vals.join(", ")
                ))
            } else {
                None
            }
        }
        (
            CustomFieldUpdate::State { name, value },
            Some(CustomField::State {
                value: actual_val, ..
            }),
        ) => {
            if actual_val.as_deref().unwrap_or("") != value {
                Some(format!(
                    "Field '{}': expected '{}', got '{}'",
                    name,
                    value,
                    actual_val.as_deref().unwrap_or("None")
                ))
            } else {
                None
            }
        }
        (
            CustomFieldUpdate::SingleUser { name, login },
            Some(CustomField::SingleUser {
                login: actual_login,
                ..
            }),
        ) => {
            if actual_login.as_deref().unwrap_or("") != login {
                Some(format!(
                    "Field '{}': expected user '{}', got '{}'",
                    name,
                    login,
                    actual_login.as_deref().unwrap_or("None")
                ))
            } else {
                None
            }
        }
        (
            CustomFieldUpdate::MultiEnum { name, values },
            Some(CustomField::SingleEnum {
                value: actual_val, ..
            }),
        ) => {
            // Backend might have mapped multi to single if only one value sent
            if values.len() == 1 {
                if actual_val.as_deref().unwrap_or("") != values[0] {
                    Some(format!(
                        "Field '{}': expected '{}', got '{}'",
                        name,
                        values[0],
                        actual_val.as_deref().unwrap_or("None")
                    ))
                } else {
                    None
                }
            } else {
                // For true multi-enum, we'd need a MultiEnum variant in CustomField (which tracker-core currently lacks, it usually flattens)
                None
            }
        }
        (_, None) => Some(format!(
            "Field '{}': update was ignored by the server",
            req_name
        )),
        _ => None, // Type mismatch or other case — already handled by build_custom_fields or server
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
            ProjectCustomField {
                id: "4".to_string(),
                name: "Platform".to_string(),
                field_type: "enum[*]".to_string(),
                required: true,
                values: vec![
                    "Windows".to_string(),
                    "macOS".to_string(),
                    "Linux".to_string(),
                ],
                state_values: vec![],
            },
        ];

        let fields = vec![
            "Phase=Planning".to_string(),
            "System=Web".to_string(),
            "Reviewer=alice".to_string(),
            "Platform=Windows, macOS".to_string(),
        ];
        let result = build_custom_fields(&fields, None, None, None, Some(&schema)).unwrap();

        assert_eq!(result.len(), 4);
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
        match &result[3] {
            CustomFieldUpdate::MultiEnum { name, values } => {
                assert_eq!(name, "Platform");
                assert_eq!(values, &["Windows", "macOS"]);
            }
            other => panic!("Platform should be detected as MultiEnum, got: {:?}", other),
        }
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
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Either a search query")
        );
    }

    #[test]
    fn create_payload_deserializes_parent() {
        let json = r#"{"project":"PROJ","summary":"Child","parent":"PROJ-100"}"#;
        let parsed: CreateIssuePayload = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.parent.as_deref(), Some("PROJ-100"));
    }

    #[test]
    fn create_payload_parent_defaults_to_none() {
        let json = r#"{"project":"PROJ","summary":"Regular"}"#;
        let parsed: CreateIssuePayload = serde_json::from_str(json).unwrap();
        assert!(parsed.parent.is_none());
    }

    #[test]
    fn update_payload_deserializes_parent() {
        let json = r#"{"parent":"PROJ-200"}"#;
        let result = parse_update_payload(json);
        assert!(result.is_ok());
        let update = result.unwrap();
        assert_eq!(update.parent.as_deref(), Some("PROJ-200"));
    }

    #[test]
    fn update_payload_parent_alone_is_valid() {
        // parent alone should satisfy the "at least one field" requirement
        let json = r#"{"parent":"PROJ-100"}"#;
        let result = parse_update_payload(json);
        assert!(result.is_ok());
    }

    #[test]
    fn update_payload_empty_still_rejected() {
        let json = r#"{}"#;
        let result = parse_update_payload(json);
        assert!(result.is_err());
    }

    #[test]
    fn build_custom_fields_warns_on_unknown_field_with_schema() {
        use tracker_core::ProjectCustomField;

        let schema = vec![ProjectCustomField {
            id: "1".to_string(),
            name: "Priority".to_string(),
            field_type: "enum[1]".to_string(),
            required: false,
            values: vec![],
            state_values: vec![],
        }];

        // "Story Points" is not in the schema, should still succeed but with a warning
        let fields = vec!["Story Points=5".to_string()];
        let result = build_custom_fields(&fields, None, None, None, Some(&schema));
        // Should still succeed (warning goes to stderr, not an error)
        assert!(result.is_ok());
        let updates = result.unwrap();
        assert_eq!(updates.len(), 1);
        assert!(
            matches!(&updates[0], CustomFieldUpdate::SingleEnum { name, value }
                if name == "Story Points" && value == "5"
            ),
        );
    }

    #[test]
    fn build_custom_fields_no_warning_without_schema() {
        // Without schema, fields should pass through silently (no schema to check against)
        let fields = vec!["Story Points=5".to_string()];
        let result = build_custom_fields(&fields, None, None, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn verifies_issue_update_success() {
        use tracker_core::{CustomField, ProjectRef};

        let requested = UpdateIssue {
            summary: Some("New Title".to_string()),
            custom_fields: vec![CustomFieldUpdate::State {
                name: "State".to_string(),
                value: "Done".to_string(),
            }],
            ..Default::default()
        };

        let result = Issue {
            id: "1".into(),
            id_readable: "PROJ-1".into(),
            summary: "New Title".into(),
            description: None,
            project: ProjectRef {
                id: "P".into(),
                name: None,
                short_name: None,
            },
            custom_fields: vec![CustomField::State {
                name: "State".to_string(),
                value: Some("Done".to_string()),
                is_resolved: true,
            }],
            tags: vec![],
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
        };

        let warnings = verify_issue_update(&requested, &result);
        assert!(warnings.is_empty());
    }

    #[test]
    fn detects_verification_mismatch() {
        use tracker_core::{CustomField, ProjectRef};

        let requested = UpdateIssue {
            summary: Some("New Title".to_string()),
            custom_fields: vec![CustomFieldUpdate::State {
                name: "State".to_string(),
                value: "Done".to_string(),
            }],
            ..Default::default()
        };

        // Result still has old summary and state
        let result = Issue {
            id: "1".into(),
            id_readable: "PROJ-1".into(),
            summary: "Old Title".into(),
            description: None,
            project: ProjectRef {
                id: "P".into(),
                name: None,
                short_name: None,
            },
            custom_fields: vec![CustomField::State {
                name: "State".to_string(),
                value: Some("Open".to_string()),
                is_resolved: false,
            }],
            tags: vec![],
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
        };

        let warnings = verify_issue_update(&requested, &result);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("Summary"));
        assert!(warnings[1].contains("State"));
    }
}
