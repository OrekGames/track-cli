//! `track prime`: a compact, offline briefing for an AI agent.
//!
//! Everything comes from the cache and the config, so the command can run as
//! a session-start hook: it never touches the network. The block names the
//! project's real states, fields, values and users, which is what an agent
//! otherwise has to discover with extra commands or guess.

use crate::cache::{CachedField, TrackerCache};
use crate::cli::{Backend, OutputFormat};
use crate::config::{WorkflowPackState, load_workflow_pack, pack_query_named};
use crate::output::output_json;
use anyhow::Result;
use chrono::Duration;
use serde::Serialize;
use std::path::Path;

/// Enum values listed per field before the rest is summarised.
pub const DEFAULT_MAX_VALUES: usize = 12;
/// Assignable users listed before the rest is summarised.
const MAX_USERS: usize = 15;
/// Tags listed before the rest is summarised.
const MAX_TAGS: usize = 20;
/// Project keys listed when no project is selected.
const MAX_PROJECTS: usize = 20;
/// Longest single cached value printed. Longer ones are cut with an ellipsis.
const MAX_VALUE_CHARS: usize = 60;
/// A cache older than this gets a warning line.
const STALE_AFTER_HOURS: i64 = 24;

pub struct PrimeOptions<'a> {
    pub project: Option<&'a str>,
    pub max_values: usize,
    pub format: OutputFormat,
    pub backend: Backend,
    /// Tracker URL from the config, if it has one
    pub url: Option<&'a str>,
    pub default_project: Option<&'a str>,
    pub explicit_config: Option<&'a Path>,
}

/// A list cut to a budget, with the number of entries left out.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Capped {
    pub shown: Vec<String>,
    /// Entries not shown. Never dropped silently: the text names a command
    /// that lists them.
    pub more: usize,
}

impl Capped {
    fn new<I: IntoIterator<Item = String>>(items: I, max: usize) -> Self {
        let mut shown: Vec<String> = Vec::new();
        let mut more = 0;
        for item in items {
            if shown.len() < max {
                shown.push(item);
            } else {
                more += 1;
            }
        }
        Self { shown, more }
    }

    fn is_empty(&self) -> bool {
        self.shown.is_empty()
    }

    /// `a, b, … +N more (hint)`
    fn render(&self, hint: &str) -> String {
        let mut line = self.shown.join(", ");
        if self.more > 0 {
            line.push_str(&format!(", … +{} more ({})", self.more, hint));
        }
        line
    }
}

#[derive(Debug, Serialize)]
pub struct PrimeState {
    pub name: String,
    pub resolved: bool,
}

#[derive(Debug, Serialize)]
pub struct PrimeStateField {
    pub field: String,
    pub states: Vec<PrimeState>,
}

#[derive(Debug, Serialize)]
pub struct PrimeField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub required: bool,
    pub values: Capped,
}

#[derive(Debug, Serialize)]
pub struct PrimeTemplate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct PrimeProject {
    pub key: String,
    pub name: String,
    pub is_default: bool,
}

/// What `track prime` prints, as data. `-o json` emits this as is.
#[derive(Debug, Serialize)]
pub struct PrimeContext {
    pub backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<PrimeProject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_age_seconds: Option<i64>,
    pub stale: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub state_fields: Vec<PrimeStateField>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_fields: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<PrimeField>,
    #[serde(skip_serializing_if = "Capped::is_empty")]
    pub users: Capped,
    #[serde(skip_serializing_if = "Capped::is_empty")]
    pub tags: Capped,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<PrimeTemplate>,
    /// Keys of the cached projects, listed when none is selected.
    #[serde(skip_serializing_if = "Capped::is_empty")]
    pub projects: Capped,
    pub commands: Vec<String>,
}

pub fn handle_prime(options: &PrimeOptions<'_>) -> Result<()> {
    if let Some(mock_dir) = tracker_mock::get_mock_dir() {
        tracker_mock::log_cli_command(&mock_dir, "prime", &[]);
    }

    let mut cache = TrackerCache::load(None).unwrap_or_default();
    let _ = cache.ensure_projects();
    let _ = cache.ensure_backend_shards();

    let context = build_context(&mut cache, options);

    match options.format {
        OutputFormat::Json => output_json(&context)?,
        OutputFormat::Text => print!("{}", render_text(&context)),
    }
    Ok(())
}

/// The briefing as text, from a cache the caller already loaded. This is
/// what `track context` prints under `-o agent`.
pub(crate) fn briefing(cache: &mut TrackerCache, options: &PrimeOptions<'_>) -> String {
    let _ = cache.ensure_projects();
    let _ = cache.ensure_backend_shards();
    render_text(&build_context(cache, options))
}

/// Make a cached string safe to print on one line of an agent's context.
///
/// The cache can come from anywhere a `.tracker-cache/` directory can: a
/// tracker anyone may write to, or a cloned repository that committed one. So
/// control characters (newlines included) become spaces, runs of whitespace
/// collapse, and long values are cut.
pub(crate) fn clean(value: &str, max_chars: usize) -> String {
    let mut cleaned = String::with_capacity(value.len());
    let mut last_was_space = true;
    for ch in value.chars() {
        let ch = if ch.is_control() || ch.is_whitespace() {
            ' '
        } else {
            ch
        };
        if ch == ' ' {
            if !last_was_space {
                cleaned.push(' ');
            }
            last_was_space = true;
        } else {
            cleaned.push(ch);
            last_was_space = false;
        }
    }
    let cleaned = cleaned.trim_end();
    if cleaned.chars().count() <= max_chars {
        return cleaned.to_string();
    }
    let mut cut: String = cleaned.chars().take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

fn clean_value(value: &str) -> String {
    clean(value, MAX_VALUE_CHARS)
}

/// `enum[1]` reads as `enum`, `enum[*]` as `enum, multi`.
fn field_type_label(field_type: &str) -> String {
    let field_type = clean_value(field_type);
    if let Some(base) = field_type.strip_suffix("[*]") {
        format!("{base}, multi")
    } else if let Some(base) = field_type.strip_suffix("[1]") {
        base.to_string()
    } else {
        field_type
    }
}

fn is_user_field(field: &CachedField) -> bool {
    field.field_type.to_ascii_lowercase().contains("user")
}

fn build_context(cache: &mut TrackerCache, options: &PrimeOptions<'_>) -> PrimeContext {
    let backend = options.backend.to_string();
    let mut warnings = Vec::new();

    let cache_backend = cache
        .backend_metadata
        .as_ref()
        .map(|meta| meta.backend_type.clone());
    let cache_usable = match &cache_backend {
        None => {
            warnings.push(
                "no cached project data yet: run `track cache refresh` once, then `track prime`"
                    .to_string(),
            );
            false
        }
        Some(cached) if !cached.eq_ignore_ascii_case(&backend) => {
            warnings.push(format!(
                "the cache here was written for {}: run `track cache refresh` to rebuild it for {}",
                clean_value(cached),
                backend
            ));
            false
        }
        Some(_) => true,
    };

    let age = cache_usable.then(|| cache.age()).flatten();
    let stale = age.is_some_and(|age| age > Duration::hours(STALE_AFTER_HOURS));
    if stale {
        warnings.push(format!(
            "cache was last refreshed {}: run `track cache refresh` before relying on it",
            cache.age_string()
        ));
    }

    let url = options
        .url
        .map(str::to_string)
        .or_else(|| {
            cache
                .backend_metadata
                .as_ref()
                .map(|meta| meta.base_url.clone())
                .filter(|url| url != "unknown")
        })
        .map(|url| clean(&url, 120));

    let mut context = PrimeContext {
        backend: backend.clone(),
        url,
        project: None,
        current_user: None,
        cache_updated_at: cache_usable.then(|| cache.updated_at.clone()).flatten(),
        cache_age_seconds: age.map(|age| age.num_seconds()),
        stale,
        warnings,
        state_fields: Vec::new(),
        required_fields: Vec::new(),
        fields: Vec::new(),
        users: Capped::new([], 0),
        tags: Capped::new([], 0),
        templates: Vec::new(),
        projects: Capped::new([], 0),
        commands: Vec::new(),
    };

    if !cache_usable {
        context.commands = example_commands(options.backend, None, &[]);
        return context;
    }

    context.current_user = cache
        .current_user
        .as_ref()
        .map(|user| clean_value(user.login.as_deref().unwrap_or(&user.display_name)));

    // Which project: the flag, then the configured default, then the only one.
    let default_project = options.default_project;
    let wanted = options.project.or(default_project);
    let project = match wanted {
        Some(wanted) => cache
            .projects
            .iter()
            .find(|p| p.short_name.eq_ignore_ascii_case(wanted) || p.id == wanted)
            .cloned(),
        None if cache.projects.len() == 1 => cache.projects.first().cloned(),
        None => None,
    };

    if project.is_none() {
        if let Some(wanted) = wanted {
            context.warnings.push(format!(
                "project {} is not in the cache: run `track cache refresh`",
                clean_value(wanted)
            ));
        }
        context.projects = Capped::new(
            cache.projects.iter().map(|p| clean_value(&p.short_name)),
            MAX_PROJECTS,
        );
    }

    context.tags = Capped::new(cache.tags.iter().map(|t| clean_value(&t.name)), MAX_TAGS);

    let Some(project) = project else {
        context.commands = example_commands(options.backend, None, &[]);
        context.templates = templates(cache, options, None);
        return context;
    };

    let _ = cache.ensure_project_shard(&project.short_name);
    let key = clean_value(&project.short_name);
    context.project = Some(PrimeProject {
        key: key.clone(),
        name: clean_value(&project.name),
        is_default: default_project
            .is_some_and(|default| default.eq_ignore_ascii_case(&project.short_name)),
    });

    context.state_fields = cache
        .workflow_hints
        .iter()
        .filter(|hints| hints.project_short_name == project.short_name)
        .flat_map(|hints| hints.state_fields.iter())
        .map(|state_field| {
            let mut states: Vec<_> = state_field.states.iter().collect();
            states.sort_by_key(|state| state.ordinal);
            PrimeStateField {
                field: clean_value(&state_field.field_name),
                states: states
                    .into_iter()
                    .map(|state| PrimeState {
                        name: clean_value(&state.name),
                        resolved: state.is_resolved,
                    })
                    .collect(),
            }
        })
        .collect();

    let cached_fields = cache
        .get_project_fields(&project.short_name)
        .unwrap_or_default();
    if cached_fields.is_empty() {
        context.warnings.push(format!(
            "no field data cached for {key}: run `track cache refresh` \
             (it fetches detail for the default project only)"
        ));
    }
    context.required_fields = cached_fields
        .iter()
        .filter(|field| field.required)
        .map(|field| clean_value(&field.name))
        .collect();
    context.fields = cached_fields
        .iter()
        // State fields already have their own line.
        .filter(|field| {
            !context
                .state_fields
                .iter()
                .any(|state_field| state_field.field.eq_ignore_ascii_case(&field.name))
        })
        .map(|field| PrimeField {
            name: clean_value(&field.name),
            field_type: field_type_label(&field.field_type),
            required: field.required,
            values: if is_user_field(field) {
                // User fields list directory entries; the users line covers them.
                Capped::new([], 0)
            } else {
                Capped::new(
                    field.values.iter().map(|v| clean_value(v)),
                    options.max_values,
                )
            },
        })
        .collect();

    context.users = Capped::new(
        cache
            .get_project_users(&project.short_name)
            .unwrap_or_default()
            .iter()
            .map(|user| clean_value(user.login.as_deref().unwrap_or(&user.display_name))),
        MAX_USERS,
    );

    context.templates = templates(cache, options, Some(&project.short_name));
    context.commands = example_commands(options.backend, Some(&key), cached_fields);
    context
}

/// Query templates `-T` accepts here: the workflow pack first, then the
/// cached built-ins it does not shadow, with cached counts where known.
fn templates(
    cache: &TrackerCache,
    options: &PrimeOptions<'_>,
    project: Option<&str>,
) -> Vec<PrimeTemplate> {
    let backend = options.backend.to_string();
    // A broken pack must not break the briefing; `track workflow validate`
    // is where it gets reported.
    let pack = match load_workflow_pack(options.explicit_config) {
        Ok(WorkflowPackState::Valid(loaded)) => Some(loaded.pack),
        _ => None,
    };
    let builtins: Vec<_> = cache
        .templates_for(project)
        .iter()
        .filter(|template| template.backend.eq_ignore_ascii_case(&backend))
        .cloned()
        .collect();

    super::workflow::overlay_query_templates(pack.as_ref(), &builtins, &backend)
        .into_iter()
        .map(|template| {
            let from_pack = pack
                .as_ref()
                .is_some_and(|pack| pack_query_named(pack, &template.name).is_some());
            // Cached counts belong to the built-in of that name, not to a
            // pack query that replaced it.
            let count = project
                .filter(|_| !from_pack)
                .and_then(|project| cache.get_issue_count(project, &template.name));
            PrimeTemplate {
                name: clean_value(&template.name),
                count,
            }
        })
        .collect()
}

/// A few commands in this backend's own syntax, filled in with real names.
fn example_commands(
    backend: Backend,
    project: Option<&str>,
    fields: &[CachedField],
) -> Vec<String> {
    let key = project.unwrap_or("PROJ");
    // `--field Name=Value` for up to two required fields with known values.
    let field_flags: String = fields
        .iter()
        .filter(|field| field.required && !field.values.is_empty() && !is_state_field(field))
        .take(2)
        .map(|field| {
            format!(
                " --field {}",
                shell_quote(&format!(
                    "{}={}",
                    clean_value(&field.name),
                    clean_value(&field.values[0])
                ))
            )
        })
        .collect();

    match backend {
        Backend::YouTrack | Backend::Linear => vec![
            format!("track {key}-1 --full"),
            format!("track i s \"project: {key} #Unresolved\""),
            format!("track i new -p {key} -s \"…\"{field_flags}"),
            format!("track i start|done {key}-1 · track i cmt {key}-1 -m \"…\""),
        ],
        Backend::Jira => vec![
            format!("track {key}-1 --full"),
            format!("track i s \"project = {key} AND statusCategory != Done\""),
            format!("track i new -p {key} -s \"…\"{field_flags}"),
            format!("track i start|done {key}-1 · track i cmt {key}-1 -m \"…\""),
        ],
        Backend::GitHub => vec![
            "track i g 42 --full".to_string(),
            "track i s \"is:open label:bug\"".to_string(),
            format!("track i new -p {key} -s \"…\" --tag bug"),
            "track i done 42 · track i cmt 42 -m \"…\"".to_string(),
        ],
        Backend::GitLab => vec![
            "track i g 42 --full".to_string(),
            "track i s \"state=opened&labels=bug\"".to_string(),
            format!("track i new -p {key} -s \"…\" --tag bug"),
            "track i done 42 · track i cmt 42 -m \"…\"".to_string(),
        ],
    }
}

fn is_state_field(field: &CachedField) -> bool {
    field.field_type.to_ascii_lowercase().contains("state")
}

/// Quote `Name=Value` for a shell when it holds anything but plain words.
fn shell_quote(value: &str) -> String {
    let plain = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '=' | '-' | '_' | '.' | '/'));
    if plain {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

pub(crate) fn render_text(context: &PrimeContext) -> String {
    let mut out = String::new();
    let mut header = vec![format!("# track · {}", context.backend)];
    if let Some(url) = &context.url {
        header.push(url.clone());
    }
    if let Some(project) = &context.project {
        header.push(if project.is_default {
            format!("project {} (default)", project.key)
        } else {
            format!("project {}", project.key)
        });
    }
    if let Some(user) = &context.current_user {
        header.push(format!("you: {user}"));
    }
    if let Some(seconds) = context.cache_age_seconds {
        header.push(format!("cache {} old", age_label(seconds)));
    }
    out.push_str(&header.join(" · "));
    out.push('\n');

    for warning in &context.warnings {
        out.push_str(&format!("! {warning}\n"));
    }

    let project_key = context.project.as_ref().map(|p| p.key.as_str());

    if !context.projects.is_empty() {
        out.push_str(&format!(
            "projects: {}\n",
            context.projects.render("track p ls")
        ));
        out.push_str("no project selected: pass -p KEY, or `track config project KEY`\n");
    }

    for state_field in &context.state_fields {
        let states: Vec<String> = state_field
            .states
            .iter()
            .map(|state| {
                if state.resolved {
                    format!("{}*", state.name)
                } else {
                    state.name.clone()
                }
            })
            .collect();
        out.push_str(&format!(
            "state field: {} = {}   (* = resolved)\n",
            state_field.field,
            states.join(" → ")
        ));
    }

    if !context.required_fields.is_empty() {
        out.push_str(&format!(
            "required on create: {}\n",
            context.required_fields.join(", ")
        ));
    }

    let fields_hint = match project_key {
        Some(key) => format!("track -o json p f {key}"),
        None => "track -o json p f KEY".to_string(),
    };
    let (with_values, without_values): (Vec<_>, Vec<_>) = context
        .fields
        .iter()
        .partition(|field| !field.values.is_empty());
    if !with_values.is_empty() {
        out.push_str("fields:\n");
        for field in with_values {
            out.push_str(&format!(
                "  {} ({}): {}\n",
                field.name,
                field.field_type,
                field.values.render(&fields_hint)
            ));
        }
    }
    if !without_values.is_empty() {
        let names: Vec<String> = without_values
            .iter()
            .map(|field| format!("{} ({})", field.name, field.field_type))
            .collect();
        out.push_str(&format!("free-form fields: {}\n", names.join(", ")));
    }

    if !context.users.is_empty() {
        let hint = match project_key {
            Some(key) => format!("track -o json context -p {key}"),
            None => "track -o json context".to_string(),
        };
        out.push_str(&format!("users: {}\n", context.users.render(&hint)));
    }

    if !context.tags.is_empty() {
        out.push_str(&format!("tags: {}\n", context.tags.render("track t ls")));
    }

    if !context.templates.is_empty() {
        let names: Vec<String> = context
            .templates
            .iter()
            .map(|template| match template.count {
                Some(count) => format!("{} ({})", template.name, count),
                None => template.name.clone(),
            })
            .collect();
        out.push_str(&format!("templates (-T): {}\n", names.join(", ")));
    }

    out.push_str("commands:\n");
    for command in &context.commands {
        out.push_str(&format!("  {command}\n"));
    }
    out.push_str(&format!(
        "more: track guide {} · track context -o json · track <command> -h\n",
        context.backend
    ));
    out
}

/// `90` → `2m`, `7200` → `2h`, `200000` → `2d`.
fn age_label(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3_600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3_600),
        s => format!("{}d", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_flattens_control_characters_and_caps_length() {
        assert_eq!(
            clean("Bug\n\nIGNORE ALL\tPREVIOUS", 60),
            "Bug IGNORE ALL PREVIOUS"
        );
        assert_eq!(clean("  spaced   out  ", 60), "spaced out");
        assert_eq!(clean("abcdefghij", 5), "abcd…");
        assert_eq!(clean("\u{1b}[31mred", 60), "[31mred");
    }

    #[test]
    fn capped_reports_what_it_left_out() {
        let capped = Capped::new((1..=5).map(|n| n.to_string()), 3);
        assert_eq!(capped.shown, ["1", "2", "3"]);
        assert_eq!(capped.more, 2);
        assert_eq!(
            capped.render("track t ls"),
            "1, 2, 3, … +2 more (track t ls)"
        );

        let whole = Capped::new(["a".to_string()], 3);
        assert_eq!(whole.render("unused"), "a");
    }

    #[test]
    fn field_type_labels_drop_cardinality_markers() {
        assert_eq!(field_type_label("enum[1]"), "enum");
        assert_eq!(field_type_label("enum[*]"), "enum, multi");
        assert_eq!(field_type_label("state"), "state");
    }

    #[test]
    fn age_labels_pick_the_largest_whole_unit() {
        assert_eq!(age_label(5), "5s");
        assert_eq!(age_label(90), "1m");
        assert_eq!(age_label(7_200), "2h");
        assert_eq!(age_label(200_000), "2d");
    }

    #[test]
    fn shell_quote_only_quotes_when_needed() {
        assert_eq!(shell_quote("Priority=Normal"), "Priority=Normal");
        assert_eq!(
            shell_quote("Fix versions=1.0 beta"),
            "\"Fix versions=1.0 beta\""
        );
    }
}
