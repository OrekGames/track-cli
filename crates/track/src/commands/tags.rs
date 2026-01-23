use crate::cli::{OutputFormat, TagCommands};
use crate::output::output_list;
use anyhow::{Context, Result};
use tracker_core::IssueTracker;

pub fn handle_tags(
    client: &dyn IssueTracker,
    action: &TagCommands,
    format: OutputFormat,
) -> Result<()> {
    match action {
        TagCommands::List => handle_list(client, format),
    }
}

fn handle_list(client: &dyn IssueTracker, format: OutputFormat) -> Result<()> {
    let tags = client.list_tags().context("Failed to list tags")?;

    output_list(&tags, format);
    Ok(())
}
