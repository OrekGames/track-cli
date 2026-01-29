//! Bundle admin command handlers

use crate::cli::{BundleCommands, OutputFormat};
use crate::output::output_result;
use anyhow::{anyhow, Context, Result};
use tracker_core::{BundleType, CreateBundle, CreateBundleValue, IssueTracker};

pub fn handle_bundle(
    client: &dyn IssueTracker,
    action: &BundleCommands,
    format: OutputFormat,
) -> Result<()> {
    match action {
        BundleCommands::List { bundle_type } => handle_list(client, bundle_type, format),
        BundleCommands::Create {
            name,
            bundle_type,
            values,
            resolved,
        } => handle_create(client, name, bundle_type, values, resolved, format),
        BundleCommands::AddValue {
            bundle_id,
            bundle_type,
            value,
            resolved,
        } => handle_add_value(client, bundle_id, bundle_type, value, *resolved, format),
    }
}

fn handle_list(client: &dyn IssueTracker, bundle_type: &str, format: OutputFormat) -> Result<()> {
    let bt = BundleType::parse(bundle_type).ok_or_else(|| {
        anyhow!(
            "Invalid bundle type: {}. Valid types: enum, state, ownedField, version, build",
            bundle_type
        )
    })?;

    let bundles = client
        .list_bundles(bt)
        .context("Failed to list bundles")?;

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&bundles)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            if bundles.is_empty() {
                println!("No {} bundles found.", bundle_type);
            } else {
                for bundle in bundles {
                    println!("{} ({})", bundle.name, bundle.id);
                    if !bundle.values.is_empty() {
                        let value_names: Vec<&str> =
                            bundle.values.iter().map(|v| v.name.as_str()).collect();
                        println!("  Values: {}", value_names.join(", "));
                    }
                    println!();
                }
            }
        }
    }

    Ok(())
}

fn handle_create(
    client: &dyn IssueTracker,
    name: &str,
    bundle_type: &str,
    values: &[String],
    resolved: &[String],
    format: OutputFormat,
) -> Result<()> {
    let bt = BundleType::parse(bundle_type).ok_or_else(|| {
        anyhow!(
            "Invalid bundle type: {}. Valid types: enum, state",
            bundle_type
        )
    })?;

    let bundle_values: Vec<CreateBundleValue> = values
        .iter()
        .enumerate()
        .map(|(i, v)| CreateBundleValue {
            name: v.clone(),
            description: None,
            is_resolved: if bt == BundleType::State {
                Some(resolved.contains(v))
            } else {
                None
            },
            ordinal: Some(i as i32),
        })
        .collect();

    let create = CreateBundle {
        name: name.to_string(),
        bundle_type: bt,
        values: bundle_values,
    };

    let bundle = client
        .create_bundle(&create)
        .context("Failed to create bundle")?;

    output_result(&bundle, format);
    Ok(())
}

fn handle_add_value(
    client: &dyn IssueTracker,
    bundle_id: &str,
    bundle_type: &str,
    value: &str,
    resolved: bool,
    format: OutputFormat,
) -> Result<()> {
    let bt = BundleType::parse(bundle_type).ok_or_else(|| {
        anyhow!(
            "Invalid bundle type: {}. Valid types: enum, state",
            bundle_type
        )
    })?;

    let create_value = CreateBundleValue {
        name: value.to_string(),
        description: None,
        is_resolved: if bt == BundleType::State {
            Some(resolved)
        } else {
            None
        },
        ordinal: None, // Server will assign
    };

    let created = client
        .add_bundle_values(bundle_id, bt, &[create_value])
        .context("Failed to add value to bundle")?;

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&created)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            use colored::Colorize;
            if let Some(v) = created.first() {
                println!(
                    "{} Added value '{}' to bundle (id: {})",
                    "✓".green().bold(),
                    v.name.cyan(),
                    v.id
                );
            }
        }
    }

    Ok(())
}
