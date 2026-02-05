//! Custom field admin command handlers

use crate::cli::{FieldCommands, OutputFormat};
use crate::output::output_result;
use anyhow::{anyhow, Context, Result};
use tracker_core::{
    AttachFieldToProject, BundleType, CreateBundle, CreateBundleValue, CreateCustomField,
    CustomFieldDefinition, CustomFieldType, IssueTracker,
};

pub fn handle_field(
    client: &dyn IssueTracker,
    action: &FieldCommands,
    format: OutputFormat,
) -> Result<()> {
    match action {
        FieldCommands::List => handle_list(client, format),
        FieldCommands::Create { name, field_type } => {
            handle_create(client, name, field_type, format)
        }
        FieldCommands::New {
            name,
            field_type,
            project,
            values,
            resolved,
            required,
        } => handle_new(
            client, name, field_type, project, values, resolved, *required, format,
        ),
    }
}

fn handle_list(client: &dyn IssueTracker, format: OutputFormat) -> Result<()> {
    let fields = client
        .list_custom_field_definitions()
        .context("Failed to list custom field definitions")?;

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&fields)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            if fields.is_empty() {
                println!("No custom fields defined.");
            } else {
                println!("{:<40} {:<20} NAME", "ID", "TYPE");
                println!("{}", "-".repeat(80));
                for field in fields {
                    println!("{:<40} {:<20} {}", field.id, field.field_type, field.name);
                }
            }
        }
    }

    Ok(())
}

fn handle_create(
    client: &dyn IssueTracker,
    name: &str,
    field_type: &str,
    format: OutputFormat,
) -> Result<()> {
    let ft = CustomFieldType::parse(field_type)
        .ok_or_else(|| anyhow!("Invalid field type: {}. Valid types: enum, multi-enum, state, text, date, integer, float, period", field_type))?;

    let create = CreateCustomField {
        name: name.to_string(),
        field_type: ft,
    };

    let field = client
        .create_custom_field(&create)
        .context("Failed to create custom field")?;

    output_result(&field, format);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_new(
    client: &dyn IssueTracker,
    name: &str,
    field_type_str: &str,
    project: &str,
    values: &[String],
    resolved: &[String],
    required: bool,
    format: OutputFormat,
) -> Result<()> {
    // Parse field type
    let field_type = CustomFieldType::parse(field_type_str).ok_or_else(|| {
        anyhow!(
            "Invalid field type: {}. Valid types: enum, state",
            field_type_str
        )
    })?;

    // Determine bundle type based on field type
    let bundle_type = match field_type {
        CustomFieldType::SingleEnum | CustomFieldType::MultiEnum => BundleType::Enum,
        CustomFieldType::State => BundleType::State,
        _ => {
            return Err(anyhow!(
                "Field type '{}' does not support bundles. Use 'track field create' for this type.",
                field_type_str
            ))
        }
    };

    // Resolve project ID
    let project_id = client
        .resolve_project_id(project)
        .with_context(|| format!("Failed to resolve project '{}'", project))?;

    // Step 1: Create bundle with values
    let bundle_name = format!("{} values", name);
    let bundle_values: Vec<CreateBundleValue> = values
        .iter()
        .enumerate()
        .map(|(i, v)| CreateBundleValue {
            name: v.clone(),
            description: None,
            is_resolved: if bundle_type == BundleType::State {
                Some(resolved.contains(v))
            } else {
                None
            },
            ordinal: Some(i as i32),
        })
        .collect();

    let create_bundle = CreateBundle {
        name: bundle_name,
        bundle_type,
        values: bundle_values,
    };

    let bundle = client
        .create_bundle(&create_bundle)
        .context("Failed to create bundle")?;

    // Step 2: Create custom field
    let create_field = CreateCustomField {
        name: name.to_string(),
        field_type,
    };

    let field = client
        .create_custom_field(&create_field)
        .context("Failed to create custom field")?;

    // Step 3: Attach field to project
    let attachment = AttachFieldToProject {
        field_id: field.id.clone(),
        bundle_id: Some(bundle.id.clone()),
        can_be_empty: !required,
        empty_field_text: None,
        field_type: Some(field_type),
        bundle_type: Some(bundle_type),
    };

    let attached = client
        .attach_field_to_project(&project_id, &attachment)
        .context("Failed to attach field to project")?;

    // Output result
    #[derive(serde::Serialize)]
    struct NewFieldResult {
        field: CustomFieldDefinition,
        bundle_id: String,
        attached_to_project: String,
        values: Vec<String>,
    }

    let result = NewFieldResult {
        field,
        bundle_id: bundle.id,
        attached_to_project: project.to_string(),
        values: values.to_vec(),
    };

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&result)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            use colored::Colorize;
            println!(
                "{} Created field '{}' with {} values and attached to project '{}'",
                "✓".green().bold(),
                attached.name.cyan(),
                values.len(),
                project.cyan()
            );
            println!("  Field ID: {}", result.field.id);
            println!("  Bundle ID: {}", result.bundle_id);
            println!("  Values: {}", values.join(", "));
        }
    }

    Ok(())
}
