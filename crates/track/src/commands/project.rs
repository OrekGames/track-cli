use crate::cli::{OutputFormat, ProjectCommands};
use crate::output::{output_list, output_result};
use anyhow::{Context, Result};
use tracker_core::{AttachFieldToProject, CreateProject, IssueTracker};

pub fn handle_project(
    client: &dyn IssueTracker,
    action: &ProjectCommands,
    format: OutputFormat,
) -> Result<()> {
    match action {
        ProjectCommands::List => handle_list(client, format),
        ProjectCommands::Get { id } => handle_get(client, id, format),
        ProjectCommands::Create {
            name,
            short_name,
            description,
        } => handle_create(client, name, short_name, description.clone(), format),
        ProjectCommands::Fields { id } => handle_fields(client, id, format),
        ProjectCommands::AttachField {
            project,
            field,
            bundle,
            required,
            empty_text,
        } => handle_attach_field(
            client,
            project,
            field,
            bundle.as_deref(),
            *required,
            empty_text.clone(),
            format,
        ),
    }
}

fn handle_list(client: &dyn IssueTracker, format: OutputFormat) -> Result<()> {
    let projects = client.list_projects().context("Failed to list projects")?;

    output_list(&projects, format);
    Ok(())
}

fn handle_create(
    client: &dyn IssueTracker,
    name: &str,
    short_name: &str,
    description: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let create = CreateProject {
        name: name.to_string(),
        short_name: short_name.to_string(),
        description,
    };

    let project = client
        .create_project(&create)
        .context("Failed to create project")?;

    output_result(&project, format);
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

fn handle_attach_field(
    client: &dyn IssueTracker,
    project: &str,
    field_id: &str,
    bundle_id: Option<&str>,
    required: bool,
    empty_text: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    // Resolve project shortName to internal ID if needed
    let project_id = client
        .resolve_project_id(project)
        .with_context(|| format!("Failed to resolve project '{}'", project))?;

    let attachment = AttachFieldToProject {
        field_id: field_id.to_string(),
        bundle_id: bundle_id.map(String::from),
        can_be_empty: !required,
        empty_field_text: empty_text,
        field_type: None,  // Will default to EnumProjectCustomField
        bundle_type: None, // Will default to EnumBundle
    };

    let attached = client
        .attach_field_to_project(&project_id, &attachment)
        .with_context(|| {
            format!(
                "Failed to attach field '{}' to project '{}'",
                field_id, project
            )
        })?;

    output_result(&attached, format);
    Ok(())
}
