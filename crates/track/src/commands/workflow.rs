use crate::cache::{CachedQueryTemplate, TrackerCache};
use crate::cli::{Backend, OutputFormat, WorkflowCommands};
use crate::config::{
    LoadedWorkflowPack, WorkflowPack, WorkflowPackState, load_workflow_pack, pack_query_named,
    require_valid_workflow_pack,
};
use crate::output::output_json;
use anyhow::{Result, anyhow};
use serde::Serialize;
use std::path::Path;
use tracker_core::unicode_eq_ignore_case;

#[derive(Serialize)]
struct WorkflowListItem {
    name: String,
    description: String,
    query: String,
    source: String,
    shadows_builtin: bool,
}

#[derive(Serialize)]
struct WorkflowShowQuery {
    name: String,
    description: String,
    query: String,
}

#[derive(Serialize)]
struct WorkflowShowPack {
    source: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_project: Option<String>,
    queries: Vec<WorkflowShowQuery>,
}

#[derive(Serialize)]
struct WorkflowValidateWarning {
    code: String,
    query: String,
    message: String,
}

#[derive(Serialize)]
struct WorkflowValidateReport {
    valid: bool,
    source: Option<String>,
    errors: Vec<String>,
    warnings: Vec<WorkflowValidateWarning>,
}

pub fn handle_workflow(
    action: &WorkflowCommands,
    format: OutputFormat,
    explicit_config: Option<&Path>,
    backend: Backend,
) -> Result<()> {
    match action {
        WorkflowCommands::List => handle_list(format, explicit_config, backend),
        WorkflowCommands::Show => handle_show(format, explicit_config),
        WorkflowCommands::Validate => handle_validate(format, explicit_config, backend),
    }
}

fn handle_list(
    format: OutputFormat,
    explicit_config: Option<&Path>,
    backend: Backend,
) -> Result<()> {
    let loaded = require_valid_workflow_pack(explicit_config)?;
    let compiled = TrackerCache::get_query_templates(&backend.to_string());
    let cached = load_cached_templates();
    let items = list_items(loaded.as_ref(), cached.as_deref(), &compiled);

    match format {
        OutputFormat::Json => output_json(&items)?,
        OutputFormat::Text => {
            if items.is_empty() {
                println!("No workflow pack queries or cached built-in templates.");
            } else {
                println!("Workflow queries:");
                for item in &items {
                    let shadow = if item.shadows_builtin {
                        " (shadows built-in)"
                    } else {
                        ""
                    };
                    println!("  {}  [{}]{}", item.name, item.source, shadow);
                    if !item.description.is_empty() {
                        println!("    {}", item.description);
                    }
                    println!("    {}", item.query);
                }
            }
            if cached.is_none() {
                println!(
                    "Built-in templates are not listed because the local cache is missing. Run 'track cache refresh'."
                );
            }
        }
    }
    Ok(())
}

fn handle_show(format: OutputFormat, explicit_config: Option<&Path>) -> Result<()> {
    let loaded = require_valid_workflow_pack(explicit_config)?;
    match format {
        OutputFormat::Json => match loaded {
            Some(loaded) => output_json(&show_pack(&loaded))?,
            None => output_json(&serde_json::Value::Null)?,
        },
        OutputFormat::Text => match loaded {
            Some(loaded) => {
                println!("Workflow pack: {} [{}]", loaded.pack.name, loaded.source);
                if let Some(description) = &loaded.pack.description {
                    println!("  {}", description);
                }
                if let Some(project) = &loaded.pack.default_project {
                    println!("  default_project: {}", project);
                }
                println!("  queries:");
                for query in &loaded.pack.queries {
                    println!("    {}: {}", query.name, query.description);
                    println!("      {}", query.query);
                }
            }
            None => println!("No workflow pack is configured."),
        },
    }
    Ok(())
}

fn handle_validate(
    format: OutputFormat,
    explicit_config: Option<&Path>,
    backend: Backend,
) -> Result<()> {
    let state = load_workflow_pack(explicit_config)?;
    let compiled = TrackerCache::get_query_templates(&backend.to_string());
    let (valid, source, errors, warnings) = match state {
        WorkflowPackState::None => (true, None, Vec::new(), Vec::new()),
        WorkflowPackState::Valid(loaded) => {
            let warnings = shadow_warnings(&loaded.pack, &compiled);
            (true, Some(loaded.source), Vec::new(), warnings)
        }
        WorkflowPackState::Invalid { source, errors } => (false, Some(source), errors, Vec::new()),
    };

    let report = WorkflowValidateReport {
        valid,
        source: source.map(|s| s.as_str().to_string()),
        errors,
        warnings,
    };

    match format {
        OutputFormat::Json => output_json(&report)?,
        OutputFormat::Text => {
            println!(
                "valid: {}{}",
                report.valid,
                report
                    .source
                    .as_deref()
                    .map(|s| format!("  source: {s}"))
                    .unwrap_or_default()
            );
            for error in &report.errors {
                println!("error: {error}");
            }
            for warning in &report.warnings {
                println!(
                    "warning: {} ({}) {}",
                    warning.code, warning.query, warning.message
                );
            }
        }
    }

    if report.valid {
        Ok(())
    } else {
        Err(anyhow!("{}", report.errors.join("\n")))
    }
}

fn show_pack(loaded: &LoadedWorkflowPack) -> WorkflowShowPack {
    WorkflowShowPack {
        source: loaded.source.as_str().to_string(),
        name: loaded.pack.name.clone(),
        description: loaded.pack.description.clone(),
        default_project: loaded.pack.default_project.clone(),
        queries: loaded
            .pack
            .queries
            .iter()
            .map(|query| WorkflowShowQuery {
                name: query.name.clone(),
                description: query.description.clone(),
                query: query.query.clone(),
            })
            .collect(),
    }
}

fn list_items(
    loaded: Option<&LoadedWorkflowPack>,
    cached: Option<&[CachedQueryTemplate]>,
    compiled: &[CachedQueryTemplate],
) -> Vec<WorkflowListItem> {
    let mut items = Vec::new();
    if let Some(loaded) = loaded {
        let source = loaded.source.as_str().to_string();
        for query in &loaded.pack.queries {
            items.push(WorkflowListItem {
                name: query.name.clone(),
                description: query.description.clone(),
                query: query.query.clone(),
                source: source.clone(),
                shadows_builtin: compiled
                    .iter()
                    .any(|builtin| unicode_eq_ignore_case(&builtin.name, &query.name)),
            });
        }
    }

    if let Some(cached) = cached {
        for template in cached {
            if loaded.is_some_and(|loaded| pack_query_named(&loaded.pack, &template.name).is_some())
            {
                continue;
            }
            items.push(WorkflowListItem {
                name: template.name.clone(),
                description: template.description.clone(),
                query: template.query.clone(),
                source: "builtin".to_string(),
                shadows_builtin: false,
            });
        }
    }

    items
}

fn load_cached_templates() -> Option<Vec<CachedQueryTemplate>> {
    let mut cache = TrackerCache::load(None).ok()?;
    cache.ensure_backend_shards().ok()?;
    if cache.query_templates.is_empty() {
        None
    } else {
        Some(cache.query_templates)
    }
}

fn shadow_warnings(
    pack: &WorkflowPack,
    compiled: &[CachedQueryTemplate],
) -> Vec<WorkflowValidateWarning> {
    pack.queries
        .iter()
        .filter(|query| {
            compiled
                .iter()
                .any(|builtin| unicode_eq_ignore_case(&builtin.name, &query.name))
        })
        .map(|query| WorkflowValidateWarning {
            code: "shadows_builtin".to_string(),
            query: query.name.clone(),
            message: format!("Query '{}' shadows the built-in template.", query.name),
        })
        .collect()
}

/// Pack queries first, then cached built-ins whose names are not shadowed.
pub(crate) fn overlay_query_templates(
    pack: Option<&WorkflowPack>,
    builtins: &[CachedQueryTemplate],
    backend: &str,
) -> Vec<CachedQueryTemplate> {
    let mut templates = Vec::new();
    if let Some(pack) = pack {
        for query in &pack.queries {
            templates.push(CachedQueryTemplate {
                name: query.name.clone(),
                description: query.description.clone(),
                query: query.query.clone(),
                backend: backend.to_string(),
            });
        }
    }
    for builtin in builtins {
        if pack.is_some_and(|pack| pack_query_named(pack, &builtin.name).is_some()) {
            continue;
        }
        templates.push(builtin.clone());
    }
    templates
}

/// JSON view of the selected pack for `track context`.
pub(crate) fn context_pack_json(loaded: &LoadedWorkflowPack) -> serde_json::Value {
    serde_json::json!({
        "source": loaded.source.as_str(),
        "name": loaded.pack.name,
        "description": loaded.pack.description,
        "default_project": loaded.pack.default_project,
        "queries": loaded.pack.queries.iter().map(|query| {
            serde_json::json!({
                "name": query.name,
                "description": query.description,
                "query": query.query,
            })
        }).collect::<Vec<_>>(),
    })
}
