//! `track prime`: a compact, offline briefing for an AI agent.
//!
//! Everything comes from the cache and the config, so the command can run as
//! a session-start hook: it never touches the network. The block names the
//! project's real states, fields, values and users, which is what an agent
//! otherwise has to discover with extra commands or guess.

use crate::cache::{CachedField, STALE_AFTER, TrackerCache};
use crate::cli::{AgentScope, AgentTool, Backend, HookAgent, OutputFormat};
use crate::config::{WorkflowPackState, load_workflow_pack, pack_query_named};
use crate::output::{Capped, MAX_VALUE_CHARS, OUTPUT_ENV, OUTPUT_ENV_VALUE, clean, output_json};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Enum values listed per field before the rest is summarised.
pub const DEFAULT_MAX_VALUES: usize = 12;
/// Assignable users listed before the rest is summarised.
const MAX_USERS: usize = 15;
/// Tags listed before the rest is summarised.
const MAX_TAGS: usize = 20;
/// Project keys listed when no project is selected.
const MAX_PROJECTS: usize = 20;
/// Longest tracker URL printed.
const MAX_URL_CHARS: usize = 120;

pub struct PrimeOptions<'a> {
    pub project: Option<&'a str>,
    pub max_values: usize,
    pub format: OutputFormat,
    pub backend: Backend,
    /// Tracker URL from the config, if it has one
    pub url: Option<&'a str>,
    pub default_project: Option<&'a str>,
    pub explicit_config: Option<&'a Path>,
    /// Set when running as a session-start hook: the briefing is wrapped in
    /// the JSON that agent's hook protocol reads.
    pub hook: Option<HookAgent>,
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

/// What `track prime` prints, as data. `-o json` emits this as is, so it
/// holds the cached values as the tracker has them; [`render_text`] makes
/// them safe to print.
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

    if options.hook == Some(HookAgent::Claude) && claude_session_in_cursor() {
        // Cursor also runs Claude Code's hooks; its own already briefed.
        print_empty_hook_output();
        return Ok(());
    }

    let mut cache = TrackerCache::load(None).unwrap_or_default();
    let _ = cache.ensure_projects();
    let _ = cache.ensure_backend_shards();

    let context = build_context(&mut cache, options);

    if let Some(agent) = options.hook {
        if agent == HookAgent::Claude {
            let _ = export_to_claude_env_file();
        }
        println!("{}", hook_payload(agent, &render_text(&context)));
        return Ok(());
    }

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

// ============================================================================
// Session-start hooks
// ============================================================================

/// The JSON a hook prints when it has nothing to add. Every supported agent
/// reads it as "no context", and those that require JSON on stdout (Gemini
/// CLI) get valid JSON.
pub fn print_empty_hook_output() {
    println!("{{}}");
}

/// The briefing wrapped in the session-start output each agent documents.
///
/// - Claude Code, Codex and Gemini CLI read `hookSpecificOutput.additionalContext`.
///   Orek.ai's planned hooks follow the same shape.
/// - Cursor reads `additional_context`, and `env` for the session's later
///   hooks.
/// - Antigravity injects `injectSteps`; a system message keeps the briefing
///   out of the user's turn.
pub fn hook_payload(agent: HookAgent, text: &str) -> String {
    let text = text.trim_end();
    let payload = match agent {
        HookAgent::Claude | HookAgent::Codex | HookAgent::Gemini | HookAgent::Orek => {
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": text,
                }
            })
        }
        HookAgent::Cursor => serde_json::json!({
            "additional_context": text,
            "env": { OUTPUT_ENV: OUTPUT_ENV_VALUE },
        }),
        HookAgent::Antigravity => serde_json::json!({
            "injectSteps": [{ "systemMessage": { "systemMessage": text } }],
        }),
    };
    payload.to_string()
}

/// What the agent passed the hook on stdin, as far as `track` needs it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HookInput {
    /// The session's project folder: `cwd` (Claude Code, Codex, Gemini CLI),
    /// the first of `workspace_roots` (Cursor) or of `workspacePaths`
    /// (Antigravity).
    pub dir: Option<PathBuf>,
    /// Cursor running a hook from Claude Code's settings sends its version.
    pub from_cursor: bool,
}

static HOOK_INPUT: OnceLock<HookInput> = OnceLock::new();

/// Largest hook input read. The fields used are near the start.
const MAX_HOOK_INPUT_BYTES: u64 = 1024 * 1024;

/// The keys of a hook's input that `track` reads; everything else is
/// skipped unparsed. Values are kept loose so one of an unexpected type
/// costs only that value, not the whole input.
#[derive(Deserialize)]
struct RawHookInput {
    #[serde(default)]
    cwd: Option<serde_json::Value>,
    #[serde(default)]
    workspace_roots: Option<serde_json::Value>,
    #[serde(default, rename = "workspacePaths")]
    workspace_paths: Option<serde_json::Value>,
    /// Present (with any value) when Cursor runs the hook.
    #[serde(default, deserialize_with = "present")]
    cursor_version: bool,
}

fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    serde::de::IgnoredAny::deserialize(deserializer).map(|_| true)
}

impl From<RawHookInput> for HookInput {
    fn from(raw: RawHookInput) -> Self {
        let first = |value: Option<serde_json::Value>| {
            value?.as_array()?.first()?.as_str().map(str::to_string)
        };
        let dir = raw
            .cwd
            .and_then(|cwd| cwd.as_str().map(str::to_string))
            .or_else(|| first(raw.workspace_roots))
            .or_else(|| first(raw.workspace_paths))
            .filter(|dir| !dir.is_empty())
            .map(|dir| PathBuf::from(dir.strip_prefix("file://").unwrap_or(&dir)));
        HookInput {
            dir,
            from_cursor: raw.cursor_version,
        }
    }
}

/// The first JSON object `reader` yields, read no further than its closing
/// brace: an agent that writes its input and keeps the pipe open is
/// answered at once, not when the pipe closes.
fn read_hook_input(reader: impl std::io::Read) -> HookInput {
    serde_json::Deserializer::from_reader(reader)
        .into_iter::<RawHookInput>()
        .next()
        .and_then(Result::ok)
        .map(HookInput::from)
        .unwrap_or_default()
}

#[cfg(test)]
fn parse_hook_input(text: &str) -> HookInput {
    read_hook_input(text.as_bytes())
}

/// Read the hook's stdin (when it is not a terminal) and move into the
/// session's project folder, so config and cache resolve for the project the
/// agent opened rather than wherever the agent was launched. Waits briefly
/// for an agent that sends nothing: that must not stall the session.
pub fn prepare_hook() {
    use std::io::{IsTerminal, Read};
    let input = if std::io::stdin().is_terminal() {
        HookInput::default()
    } else {
        let (sender, receiver) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().spawn(move || {
            let input = read_hook_input(std::io::stdin().lock().take(MAX_HOOK_INPUT_BYTES));
            let _ = sender.send(input);
        });
        receiver
            .recv_timeout(std::time::Duration::from_millis(500))
            .unwrap_or_default()
    };
    if let Some(dir) = &input.dir
        && dir.is_dir()
    {
        let _ = std::env::set_current_dir(dir);
    }
    let _ = HOOK_INPUT.set(input);
}

/// Whether this is Cursor running Claude Code's hook while a Cursor hook of
/// ours is also installed, which would print the briefing twice.
fn claude_session_in_cursor() -> bool {
    HOOK_INPUT.get().is_some_and(|input| input.from_cursor)
        && [AgentScope::User, AgentScope::Project]
            .into_iter()
            .any(|scope| super::agent::hook_installed(scope, AgentTool::Cursor))
}

/// Claude Code sources `CLAUDE_ENV_FILE` before each shell command of the
/// session, so this makes every later `track` call use agent output.
fn export_to_claude_env_file() -> std::io::Result<()> {
    let Some(path) = crate::config::env_path("CLAUDE_ENV_FILE") else {
        return Ok(());
    };
    let line = format!("export {OUTPUT_ENV}={OUTPUT_ENV_VALUE}");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        writeln!(file)?;
    }
    writeln!(file, "{line}")
}

fn clean_value(value: &str) -> String {
    clean(value, MAX_VALUE_CHARS)
}

/// Whether `value` prints exactly as it is, unflattened and uncut.
fn prints_whole(value: &str) -> bool {
    clean_value(value) == value
}

/// `enum[1]` reads as `enum`, `enum[*]` as `enum, multi`.
fn field_type_label(field_type: &str) -> String {
    if let Some(base) = field_type.strip_suffix("[*]") {
        format!("{base}, multi")
    } else if let Some(base) = field_type.strip_suffix("[1]") {
        base.to_string()
    } else {
        field_type.to_string()
    }
}

fn is_user_field(field: &CachedField) -> bool {
    field.field_type.to_ascii_lowercase().contains("user")
}

/// A user as the briefing names them: the login, else the display name.
fn user_name(user: &crate::cache::CachedUser) -> String {
    user.login
        .clone()
        .unwrap_or_else(|| user.display_name.clone())
}

/// The briefing's facts, with cached values as the tracker has them. The
/// warnings are sentences, so the cached text they quote is made safe here.
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
        // Another URL or repository than the config names (or a cache from
        // before track recorded one): its states and users are not this
        // tracker's.
        Some(_) if !cache.matches_active() => {
            warnings.push(format!(
                "the cache here is for another {backend} instance than this config, or from an \
                 older track: run `track cache refresh` to rebuild it"
            ));
            false
        }
        Some(_) => true,
    };

    let age = cache_usable.then(|| cache.age()).flatten();
    let stale = cache_usable && cache.is_stale(STALE_AFTER);
    if stale {
        warnings.push(match age {
            Some(age) => format!(
                "cache was last refreshed {} ago: run `track cache refresh` before relying on it",
                age_label(age.num_seconds())
            ),
            None => "the cache does not say when it was refreshed: run `track cache refresh` \
                     before relying on it"
                .to_string(),
        });
    }

    let url = options.url.map(str::to_string).or_else(|| {
        cache
            .backend_metadata
            .as_ref()
            .filter(|_| cache_usable)
            .map(|meta| meta.base_url.clone())
            .filter(|url| url != "unknown")
    });

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

    context.current_user = cache.current_user.as_ref().map(user_name);

    // Which project: the flag, then the configured default, then the only one.
    let default_project = options.default_project;
    let wanted = options.project.or(default_project);
    let project = match wanted {
        Some(wanted) => cache.find_project(wanted),
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
            cache.projects.iter().map(|p| p.short_name.clone()),
            MAX_PROJECTS,
        );
    }

    context.tags = Capped::new(cache.tags.iter().map(|t| t.name.clone()), MAX_TAGS);

    let Some(project) = project else {
        context.commands = example_commands(options.backend, None, &[]);
        context.templates = templates(cache, options, None);
        return context;
    };

    let _ = cache.ensure_project_shard(&project.short_name);
    let key = project.short_name.clone();
    context.project = Some(PrimeProject {
        key: key.clone(),
        name: project.name.clone(),
        is_default: default_project.is_some_and(|default| {
            tracker_core::unicode_eq_ignore_case(default, &project.short_name)
                || default == project.id
        }),
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
                field: state_field.field_name.clone(),
                states: states
                    .into_iter()
                    .map(|state| PrimeState {
                        name: state.name.clone(),
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
            "no field data cached for {}: run `track cache refresh` \
             (it fetches detail for the default project only)",
            clean_value(&key)
        ));
    }
    context.required_fields = cached_fields
        .iter()
        .filter(|field| field.required)
        .map(|field| field.name.clone())
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
            name: field.name.clone(),
            field_type: field_type_label(&field.field_type),
            required: field.required,
            values: if is_user_field(field) {
                // User fields list directory entries; the users line covers them.
                Capped::new([], 0)
            } else {
                Capped::new(field.values.iter().cloned(), options.max_values)
            },
        })
        .collect();

    context.users = Capped::new(
        cache
            .get_project_users(&project.short_name)
            .unwrap_or_default()
            .iter()
            .map(user_name),
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
                name: template.name,
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
    // Only ones that print whole: a flattened or cut value would make a
    // command the tracker rejects.
    let field_flags: String = fields
        .iter()
        .filter(|field| {
            field.required
                && !is_state_field(field)
                && prints_whole(&field.name)
                && field
                    .values
                    .first()
                    .is_some_and(|value| prints_whole(value))
        })
        .take(2)
        .map(|field| {
            format!(
                " --field {}",
                shell_quote(&format!("{}={}", field.name, field.values[0]))
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

/// The briefing as text. Every cached string goes through [`clean`] here,
/// where it is printed, so one stays on its line and within its length.
pub(crate) fn render_text(context: &PrimeContext) -> String {
    // Sentences and commands are flattened but never cut.
    let line = |text: &str| clean(text, usize::MAX);
    let mut out = String::new();
    let mut header = vec![format!("# track · {}", context.backend)];
    if let Some(url) = &context.url {
        header.push(clean(url, MAX_URL_CHARS));
    }
    let project_key = context.project.as_ref().map(|p| clean_value(&p.key));
    if let (Some(project), Some(key)) = (&context.project, &project_key) {
        header.push(if project.is_default {
            format!("project {key} (default)")
        } else {
            format!("project {key}")
        });
    }
    if let Some(user) = &context.current_user {
        header.push(format!("you: {}", clean_value(user)));
    }
    if let Some(seconds) = context.cache_age_seconds {
        header.push(format!("cache {} old", age_label(seconds)));
    }
    out.push_str(&header.join(" · "));
    out.push('\n');

    for warning in &context.warnings {
        out.push_str(&format!("! {}\n", line(warning)));
    }

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
                let name = clean_value(&state.name);
                if state.resolved {
                    format!("{name}*")
                } else {
                    name
                }
            })
            .collect();
        out.push_str(&format!(
            "state field: {} = {}   (* = resolved)\n",
            clean_value(&state_field.field),
            states.join(" → ")
        ));
    }

    if !context.required_fields.is_empty() {
        let names: Vec<String> = context
            .required_fields
            .iter()
            .map(|name| clean_value(name))
            .collect();
        out.push_str(&format!("required on create: {}\n", names.join(", ")));
    }

    let fields_hint = match &project_key {
        Some(key) => format!("track -o json p f {key}"),
        None => "track -o json p f KEY".to_string(),
    };
    let field_label = |field: &PrimeField| {
        format!(
            "{} ({})",
            clean_value(&field.name),
            clean_value(&field.field_type)
        )
    };
    let (with_values, without_values): (Vec<_>, Vec<_>) = context
        .fields
        .iter()
        .partition(|field| !field.values.is_empty());
    if !with_values.is_empty() {
        out.push_str("fields:\n");
        for field in with_values {
            out.push_str(&format!(
                "  {}: {}\n",
                field_label(field),
                field.values.render(&fields_hint)
            ));
        }
    }
    if !without_values.is_empty() {
        let names: Vec<String> = without_values.into_iter().map(field_label).collect();
        out.push_str(&format!("free-form fields: {}\n", names.join(", ")));
    }

    if !context.users.is_empty() {
        let hint = match &project_key {
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
            .map(|template| {
                let name = clean_value(&template.name);
                match template.count {
                    Some(count) => format!("{name} ({count})"),
                    None => name,
                }
            })
            .collect();
        out.push_str(&format!("templates (-T): {}\n", names.join(", ")));
    }

    out.push_str("commands:\n");
    for command in &context.commands {
        out.push_str(&format!("  {}\n", line(command)));
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
    fn hook_input_names_the_session_folder_by_each_agents_key() {
        assert_eq!(
            parse_hook_input(r#"{"cwd": "/p", "workspace_roots": ["/q"]}"#).dir,
            Some(PathBuf::from("/p"))
        );
        let cursor = parse_hook_input(r#"{"workspace_roots": ["/q"], "cursor_version": "2.0"}"#);
        assert_eq!(cursor.dir, Some(PathBuf::from("/q")));
        assert!(cursor.from_cursor);
        assert_eq!(
            parse_hook_input(r#"{"workspacePaths": ["file:///r"]}"#).dir,
            Some(PathBuf::from("/r"))
        );
        // A value of an unexpected type costs only that value.
        let odd = parse_hook_input(
            r#"{"cwd": 7, "workspace_roots": [3, "/q"], "workspacePaths": ["/r"], "x": {"deep": [1]}}"#,
        );
        assert_eq!(odd.dir, Some(PathBuf::from("/r")));
        assert!(!odd.from_cursor);
        assert_eq!(parse_hook_input("not json"), HookInput::default());
        assert_eq!(parse_hook_input(""), HookInput::default());
    }

    /// A reader that hands out `bytes` and then fails the test if read
    /// again, as a pipe the agent keeps open would block.
    struct OpenPipe {
        bytes: Vec<u8>,
        at: usize,
    }

    impl std::io::Read for OpenPipe {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            assert!(
                self.at < self.bytes.len(),
                "read past the end of the hook input"
            );
            let n = buf.len().min(self.bytes.len() - self.at);
            buf[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
            self.at += n;
            Ok(n)
        }
    }

    #[test]
    fn hook_input_is_read_only_up_to_the_end_of_its_object() {
        let pipe = OpenPipe {
            bytes: br#"  {"cwd": "/p", "cursor_version": null}"#.to_vec(),
            at: 0,
        };
        let input = read_hook_input(pipe);
        assert_eq!(input.dir, Some(PathBuf::from("/p")));
        assert!(input.from_cursor);
    }

    #[test]
    fn values_are_cleaned_where_printed_and_kept_whole_in_the_data() {
        let long = "x".repeat(80);
        let context = PrimeContext {
            backend: "youtrack".to_string(),
            url: None,
            project: Some(PrimeProject {
                key: "DE\nMO".to_string(),
                name: "Demo".to_string(),
                is_default: false,
            }),
            current_user: Some("jdoe\n# SYSTEM".to_string()),
            cache_updated_at: None,
            cache_age_seconds: None,
            stale: false,
            warnings: Vec::new(),
            state_fields: Vec::new(),
            required_fields: vec!["Pri\tority".to_string()],
            fields: vec![PrimeField {
                name: "Size".to_string(),
                field_type: "enum".to_string(),
                required: false,
                values: Capped::new([long.clone()], 5),
            }],
            users: Capped::new([], 0),
            tags: Capped::new([], 0),
            templates: Vec::new(),
            projects: Capped::new([], 0),
            commands: vec!["track i new -p DE\nMO".to_string()],
        };
        let text = render_text(&context);
        assert!(
            text.contains("project DE MO · you: jdoe # SYSTEM\n"),
            "{text}"
        );
        assert!(text.contains("required on create: Pri ority\n"), "{text}");
        assert!(text.contains(&format!("  Size (enum): {}…\n", "x".repeat(59))));
        assert!(text.contains("  track i new -p DE MO\n"), "{text}");
        // The data, which `-o json` prints, keeps the values whole.
        assert_eq!(context.fields[0].values.shown[0], long);
    }

    #[test]
    fn suggested_field_flags_use_only_values_that_print_whole() {
        let field = |name: &str, value: &str| CachedField {
            name: name.to_string(),
            field_id: None,
            field_type: "enum[1]".to_string(),
            required: true,
            values: vec![value.to_string()],
        };
        let commands = example_commands(
            Backend::YouTrack,
            Some("DEMO"),
            &[
                field("Area", &"a".repeat(80)),
                field("Team", "Core\nOps"),
                field("Priority", "Normal"),
            ],
        );
        assert!(
            commands.contains(&"track i new -p DEMO -s \"…\" --field Priority=Normal".to_string()),
            "{commands:?}"
        );
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
