use crate::cli::{OutputFormat, ProjectCommands};
use crate::lookup::Lookup;
use crate::output::{output_list, output_result};
use anyhow::{Context, Result};
use tracker_core::{AttachFieldToProject, CreateProject, IssueTracker};

pub fn handle_project(
    lookup: &Lookup,
    action: &ProjectCommands,
    format: OutputFormat,
) -> Result<()> {
    let client = lookup.client();
    match action {
        ProjectCommands::List => handle_list(client, format),
        ProjectCommands::Get { id } => handle_get(client, id, format),
        ProjectCommands::Create {
            name,
            short_name,
            description,
            body_file,
            leader,
        } => {
            let resolved_desc = super::resolve_body(description.as_deref(), body_file.as_deref())?;
            handle_create(
                lookup,
                name,
                short_name,
                resolved_desc,
                leader.as_deref(),
                format,
            )
        }
        ProjectCommands::Fields { id } => handle_fields(lookup, id, format),
        ProjectCommands::AttachField {
            project,
            field,
            bundle,
            required,
            empty_text,
        } => handle_attach_field(
            lookup,
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

    output_list(&projects, format)?;
    Ok(())
}

fn handle_create(
    lookup: &Lookup,
    name: &str,
    short_name: &str,
    description: Option<String>,
    leader: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    // Only an omitted --leader means "me"; an empty one (e.g. an unset shell
    // variable) is a mistake that would otherwise pick a leader silently.
    if leader.is_some_and(|leader| leader.trim().is_empty()) {
        anyhow::bail!(
            "--leader must not be empty: pass a user ID, login, or \"me\", \
             or omit --leader to make yourself the project leader"
        );
    }

    let create = CreateProject {
        name: name.to_string(),
        short_name: short_name.to_string(),
        description,
        leader: leader.map(String::from),
    };

    let project = lookup
        .client()
        .create_project(&create)
        .context("Failed to create project")?;
    lookup.record_project(&project);

    output_result(&project, format)?;
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

    output_result(&project, format)?;
    Ok(())
}

fn handle_fields(lookup: &Lookup, id: &str, format: OutputFormat) -> Result<()> {
    // The schema always comes from the tracker; the cache learns it.
    let project_id = lookup.project_id(id)?;
    let fields =
        lookup.with_project_id(&project_id, |project_id| lookup.schema_live(project_id))?;

    output_list(&fields, format)?;
    Ok(())
}

fn handle_attach_field(
    lookup: &Lookup,
    project: &str,
    field_id: &str,
    bundle_id: Option<&str>,
    required: bool,
    empty_text: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let attachment = AttachFieldToProject {
        field_id: field_id.to_string(),
        bundle_id: bundle_id.map(String::from),
        can_be_empty: !required,
        empty_field_text: empty_text,
        field_type: None,  // Will default to EnumProjectCustomField
        bundle_type: None, // Will default to EnumBundle
    };

    let project_id = lookup.project_id(project)?;
    let attached = lookup.with_project_id(&project_id, |project_id| {
        let attached = lookup
            .client()
            .attach_field_to_project(project_id, &attachment)
            .with_context(|| {
                format!(
                    "Failed to attach field '{}' to project '{}'",
                    field_id, project
                )
            })?;
        lookup.forget_schema(project_id);
        Ok(attached)
    })?;

    output_result(&attached, format)?;
    Ok(())
}
