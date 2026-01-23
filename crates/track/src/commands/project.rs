use crate::cli::{OutputFormat, ProjectCommands};
use crate::output::{output_list, output_result};
use anyhow::{Context, Result};
use tracker_core::IssueTracker;

pub fn handle_project(
    client: &dyn IssueTracker,
    action: &ProjectCommands,
    format: OutputFormat,
) -> Result<()> {
    match action {
        ProjectCommands::List => handle_list(client, format),
        ProjectCommands::Get { id } => handle_get(client, id, format),
        ProjectCommands::Fields { id } => handle_fields(client, id, format),
    }
}

fn handle_list(client: &dyn IssueTracker, format: OutputFormat) -> Result<()> {
    let projects = client
        .list_projects()
        .context("Failed to list projects")?;

    output_list(&projects, format);
    Ok(())
}

fn handle_get(client: &dyn IssueTracker, id: &str, format: OutputFormat) -> Result<()> {
    // Resolve shortName to internal ID if needed
    let project_id = client
        .resolve_project_id(id)
        .with_context(|| format!("Failed to resolve project '{}'", id))?;

    let project = client
        .get_project(&project_id)
        .with_context(|| format!("Failed to fetch project '{}'", id))?;

    output_result(&project, format);
    Ok(())
}

fn handle_fields(client: &dyn IssueTracker, id: &str, format: OutputFormat) -> Result<()> {
    // Resolve shortName to internal ID if needed
    let project_id = client
        .resolve_project_id(id)
        .with_context(|| format!("Failed to resolve project '{}'", id))?;

    let fields = client
        .get_project_custom_fields(&project_id)
        .with_context(|| format!("Failed to fetch custom fields for project '{}'", id))?;

    output_list(&fields, format);
    Ok(())
}
