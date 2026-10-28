//! Bundle admin command handlers

use crate::cli::{BundleCommands, OutputFormat};
use crate::output::{output_json, output_result};
use anyhow::{Context, Result, anyhow};
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

    let bundles = client.list_bundles(bt).context("Failed to list bundles")?;

    match format {
        OutputFormat::Json => {
            output_json(&bundles)?;
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

    let create = CreateBundle {
        name: name.to_string(),
        bundle_type: bt,
        values: build_bundle_values(values, resolved, bt)?,
    };

    let bundle = client
        .create_bundle(&create)
        .context("Failed to create bundle")?;

    output_result(&bundle, format)?;
    Ok(())
}

/// Build the ordered values of a new bundle, flagging `resolved` names on
/// state bundles.
///
/// Every resolved name must be one of `values`: a typo would otherwise
/// silently create a state bundle in which no state counts as resolved.
pub(crate) fn build_bundle_values(
    values: &[String],
    resolved: &[String],
    bundle_type: BundleType,
) -> Result<Vec<CreateBundleValue>> {
    if let Some(unknown) = resolved.iter().find(|name| !values.contains(name)) {
        return Err(anyhow!(
            "Resolved value '{}' is not one of --values ({})",
            unknown,
            values.join(", ")
        ));
    }

    Ok(values
        .iter()
        .enumerate()
        .map(|(i, v)| CreateBundleValue {
            name: v.clone(),
            description: None,
            is_resolved: (bundle_type == BundleType::State).then(|| resolved.contains(v)),
            ordinal: Some(i as i32),
        })
        .collect())
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
            output_json(&created)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn state_bundle_values_flag_resolved_and_keep_order() {
        let values = build_bundle_values(
            &names(&["Open", "Done"]),
            &names(&["Done"]),
            BundleType::State,
        )
        .unwrap();
        let summary: Vec<_> = values
            .iter()
            .map(|v| (v.name.as_str(), v.is_resolved, v.ordinal))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Open", Some(false), Some(0)),
                ("Done", Some(true), Some(1))
            ]
        );
    }

    #[test]
    fn enum_bundle_values_carry_no_resolved_flag() {
        let values = build_bundle_values(&names(&["A"]), &[], BundleType::Enum).unwrap();
        assert_eq!(values[0].is_resolved, None);
    }

    #[test]
    fn rejects_resolved_value_missing_from_values() {
        // A case typo must not create a state bundle where nothing is resolved.
        let err = build_bundle_values(
            &names(&["Open", "Done"]),
            &names(&["done"]),
            BundleType::State,
        )
        .unwrap_err();
        assert!(err.to_string().contains("'done'"), "got: {err}");
    }
}
