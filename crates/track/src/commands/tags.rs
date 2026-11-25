use crate::cli::{OutputFormat, TagCommands};
use crate::lookup::Lookup;
use crate::output::{output_list, output_result};
use anyhow::{Context, Result};
use tracker_core::CreateTag;

pub fn handle_tags(lookup: &Lookup, action: &TagCommands, format: OutputFormat) -> Result<()> {
    match action {
        TagCommands::List => handle_list(lookup, format),
        TagCommands::Create {
            name,
            tag_color,
            description,
            body_file,
        } => {
            let resolved_desc = super::resolve_body(description.as_deref(), body_file.as_deref())?;
            handle_create(
                lookup,
                name,
                tag_color.as_deref(),
                resolved_desc.as_deref(),
                format,
            )
        }
        TagCommands::Delete { name } => handle_delete(lookup, name),
        TagCommands::Update {
            name,
            new_name,
            tag_color,
            description,
            body_file,
        } => {
            let resolved_desc = super::resolve_body(description.as_deref(), body_file.as_deref())?;
            handle_update(
                lookup,
                name,
                new_name.as_deref(),
                tag_color.as_deref(),
                resolved_desc.as_deref(),
                format,
            )
        }
    }
}

fn handle_list(lookup: &Lookup, format: OutputFormat) -> Result<()> {
    let tags = lookup.client().list_tags().context("Failed to list tags")?;
    lookup.store_tags(&tags);

    output_list(&tags, format)?;
    Ok(())
}

fn handle_create(
    lookup: &Lookup,
    name: &str,
    color: Option<&str>,
    description: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let tag = CreateTag {
        name: name.to_string(),
        color: color.map(with_hash_prefix),
        description: description.map(|d| d.to_string()),
    };

    let created = lookup
        .client()
        .create_tag(&tag)
        .context("Failed to create tag")?;
    lookup.record_tag(&created, None);

    output_result(&created, format)?;
    Ok(())
}

fn handle_delete(lookup: &Lookup, name: &str) -> Result<()> {
    lookup
        .client()
        .delete_tag(name)
        .context("Failed to delete tag")?;
    lookup.record_tag_deleted(name);

    eprintln!("Deleted tag: {}", name);
    Ok(())
}

/// Accept hex colors with or without the leading `#` and always send it.
fn with_hash_prefix(color: &str) -> String {
    if color.starts_with('#') {
        color.to_string()
    } else {
        format!("#{}", color)
    }
}

fn handle_update(
    lookup: &Lookup,
    current_name: &str,
    new_name: Option<&str>,
    color: Option<&str>,
    description: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let tag = CreateTag {
        name: new_name.unwrap_or(current_name).to_string(),
        color: color.map(with_hash_prefix),
        description: description.map(|d| d.to_string()),
    };

    let updated = lookup
        .client()
        .update_tag(current_name, &tag)
        .context("Failed to update tag")?;
    lookup.record_tag(&updated, Some(current_name));

    output_result(&updated, format)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_hash_prefix_adds_missing_hash_only() {
        assert_eq!(with_hash_prefix("d73a4a"), "#d73a4a");
        assert_eq!(with_hash_prefix("#d73a4a"), "#d73a4a");
    }
}
