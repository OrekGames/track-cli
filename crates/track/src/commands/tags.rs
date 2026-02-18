use crate::cli::{OutputFormat, TagCommands};
use crate::output::{output_list, output_result};
use anyhow::{Context, Result};
use tracker_core::{CreateTag, IssueTracker};

pub fn handle_tags(
    client: &dyn IssueTracker,
    action: &TagCommands,
    format: OutputFormat,
) -> Result<()> {
    match action {
        TagCommands::List => handle_list(client, format),
        TagCommands::Create {
            name,
            tag_color,
            description,
        } => handle_create(
            client,
            name,
            tag_color.as_deref(),
            description.as_deref(),
            format,
        ),
        TagCommands::Delete { name } => handle_delete(client, name),
        TagCommands::Update {
            name,
            new_name,
            tag_color,
            description,
        } => handle_update(
            client,
            name,
            new_name.as_deref(),
            tag_color.as_deref(),
            description.as_deref(),
            format,
        ),
    }
}

fn handle_list(client: &dyn IssueTracker, format: OutputFormat) -> Result<()> {
    let tags = client.list_tags().context("Failed to list tags")?;

    output_list(&tags, format);
    Ok(())
}

fn handle_create(
    client: &dyn IssueTracker,
    name: &str,
    color: Option<&str>,
    description: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let tag = CreateTag {
        name: name.to_string(),
        color: color.map(|c| {
            if c.starts_with('#') {
                c.to_string()
            } else {
                format!("#{}", c)
            }
        }),
        description: description.map(|d| d.to_string()),
    };

    let created = client.create_tag(&tag).context("Failed to create tag")?;

    output_result(&created, format);
    Ok(())
}

fn handle_delete(client: &dyn IssueTracker, name: &str) -> Result<()> {
    client.delete_tag(name).context("Failed to delete tag")?;

    eprintln!("Deleted tag: {}", name);
    Ok(())
}

fn handle_update(
    client: &dyn IssueTracker,
    current_name: &str,
    new_name: Option<&str>,
    color: Option<&str>,
    description: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let tag = CreateTag {
        name: new_name.unwrap_or(current_name).to_string(),
        color: color.map(|c| {
            if c.starts_with('#') {
                c.to_string()
            } else {
                format!("#{}", c)
            }
        }),
        description: description.map(|d| d.to_string()),
    };

    let updated = client
        .update_tag(current_name, &tag)
        .context("Failed to update tag")?;

    output_result(&updated, format);
    Ok(())
}
