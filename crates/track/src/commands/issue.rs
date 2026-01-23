use crate::cli::{IssueCommands, OutputFormat};
use crate::local_config::LocalConfig;
use crate::output::{output_list, output_result};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use tracker_core::{CreateIssue, CustomFieldUpdate, IssueTracker, UpdateIssue};

pub fn handle_issue(
    client: &dyn IssueTracker,
    action: &IssueCommands,
    format: OutputFormat,
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
            json.as_deref(),
            format,
        ),
        IssueCommands::Update {
            id,
            summary,
            description,
            fields,
            state,
            priority,
            assignee,
            tags,
            json,
        } => handle_update(
            client,
            id,
            summary.as_deref(),
            description.as_deref(),
            fields,
            state.as_deref(),
            priority.as_deref(),
            assignee.as_deref(),
            tags,
            json.as_deref(),
            format,
        ),
        IssueCommands::Search { query, limit, skip } => {
            handle_search(client, query, *limit, *skip, format)
        }
        IssueCommands::Delete { id } => handle_delete(client, id, format),
        IssueCommands::Comment { id, text } => handle_comment(client, id, text, format),
        IssueCommands::Comments { id, limit } => handle_comments(client, id, *limit, format),
        IssueCommands::Link {
            source,
            target,
            link_type,
        } => handle_link(client, source, target, link_type, format),
        IssueCommands::Start { id, field, state } => {
            handle_state_transition(client, id, field, state, "started", format)
        }
        IssueCommands::Complete { id, field, state } => {
            handle_state_transition(client, id, field, state, "completed", format)
        }
    }
}

fn handle_get(
    client: &dyn IssueTracker,
    id: &str,
    full: bool,
    format: OutputFormat,
) -> Result<()> {
    let issue = client
        .get_issue(id)
        .with_context(|| format!("Failed to fetch issue '{}'", id))?;

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
                println!("\n  {} ({} total):", "Recent Comments".dimmed(), comments.len());
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
    json: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let create = if let Some(payload) = json {
        parse_create_payload(client, payload)?
    } else {
        // Try CLI flag first, then fall back to local config default
        let project_input = match project {
            Some(p) => p.to_string(),
            None => {
                // Try to load default project from local config
                let local_config = LocalConfig::load().ok();
                local_config
                    .and_then(|c| c.default_project_id)
                    .ok_or_else(|| {
                        anyhow!(
                            "Project is required. Use -p/--project or set a default with 'track config project <ID>'"
                        )
                    })?
            }
        };
        let summary = summary.ok_or_else(|| anyhow!("Summary is required"))?;

        // Resolve project shortName to internal ID
        let project_id = client
            .resolve_project_id(&project_input)
            .with_context(|| format!("Failed to resolve project '{}'", project_input))?;

        let custom_fields = build_custom_fields(fields, state, priority, assignee)?;

        CreateIssue {
            project_id,
            summary: summary.to_string(),
            description: description.map(|s| s.to_string()),
            custom_fields,
            tags: tags.to_vec(),
        }
    };

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
    json: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let update = if let Some(payload) = json {
        parse_update_payload(payload)?
    } else {
        let custom_fields = build_custom_fields(fields, state, priority, assignee)?;

        UpdateIssue {
            summary: summary.map(|s| s.to_string()),
            description: description.map(|s| s.to_string()),
            custom_fields,
            tags: tags.to_vec(),
        }
    };

    let issue = client
        .update_issue(id, &update)
        .with_context(|| format!("Failed to update issue '{}'", id))?;

    output_result(&issue, format);
    Ok(())
}

/// Build a list of custom field updates from CLI arguments.
fn build_custom_fields(
    fields: &[String],
    state: Option<&str>,
    priority: Option<&str>,
    assignee: Option<&str>,
) -> Result<Vec<CustomFieldUpdate>> {
    let mut custom_fields = Vec::new();

    // Parse generic field=value pairs
    for field in fields {
        let (name, value) = parse_field_value(field)?;
        custom_fields.push(CustomFieldUpdate::SingleEnum { name, value });
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
    let tags: Vec<String> = parsed
        .tags
        .into_iter()
        .filter_map(|t| t.name)
        .collect();

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
            build_custom_fields(&fields, Some("Open"), Some("Major"), Some("john")).unwrap();

        assert_eq!(result.len(), 5); // 2 fields + state + priority + assignee
    }
}

fn handle_search(
    client: &dyn IssueTracker,
    query: &str,
    limit: usize,
    skip: usize,
    format: OutputFormat,
) -> Result<()> {
    let issues = client
        .search_issues(query, limit, skip)
        .context("Failed to search issues")?;

    output_list(&issues, format);
    Ok(())
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
    let custom_field =
        if field.eq_ignore_ascii_case("State") || field.eq_ignore_ascii_case("Stage") {
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
