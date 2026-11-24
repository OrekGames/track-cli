//! Putting `track` in front of AI agents: the skill files, the session-start
//! hooks that print `track prime`, instruction blocks, and the embedded
//! reference guide.
//!
//! Every location below comes from the tool's own documentation (see
//! `docs/design/agent-integrations.md`). A tool without a documented
//! mechanism gets nothing for it rather than a guess.

use crate::cli::{AgentCommands, AgentScope, AgentTool, HookAgent, OutputFormat};
use crate::output::output_json;
use anyhow::{Context, Result, anyhow};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::value::RawValue;
use std::path::{Path, PathBuf};

/// The skill: a small core and the references it points to, by path
/// relative to the skill directory.
pub const SKILL_FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../../../agent-skills/SKILL.md"),
    ),
    (
        "references/youtrack.md",
        include_str!("../../../../agent-skills/references/youtrack.md"),
    ),
    (
        "references/jira.md",
        include_str!("../../../../agent-skills/references/jira.md"),
    ),
    (
        "references/github.md",
        include_str!("../../../../agent-skills/references/github.md"),
    ),
    (
        "references/gitlab.md",
        include_str!("../../../../agent-skills/references/gitlab.md"),
    ),
    (
        "references/linear.md",
        include_str!("../../../../agent-skills/references/linear.md"),
    ),
    (
        "references/queries.md",
        include_str!("../../../../agent-skills/references/queries.md"),
    ),
    (
        "references/issues.md",
        include_str!("../../../../agent-skills/references/issues.md"),
    ),
    (
        "references/json.md",
        include_str!("../../../../agent-skills/references/json.md"),
    ),
    (
        "references/output.md",
        include_str!("../../../../agent-skills/references/output.md"),
    ),
    (
        "references/apply.md",
        include_str!("../../../../agent-skills/references/apply.md"),
    ),
    (
        "references/articles.md",
        include_str!("../../../../agent-skills/references/articles.md"),
    ),
    (
        "references/admin.md",
        include_str!("../../../../agent-skills/references/admin.md"),
    ),
    (
        "references/config.md",
        include_str!("../../../../agent-skills/references/config.md"),
    ),
    (
        "references/cache.md",
        include_str!("../../../../agent-skills/references/cache.md"),
    ),
];

/// The core skill must stay small: it is loaded whole whenever the skill is
/// used, while references are read one at a time. Orek.ai caps a skill's
/// instructions at 8,000 bytes; this keeps well inside that.
#[cfg(test)]
const CORE_SKILL_BUDGET_BYTES: usize = 6 * 1024;

/// Every hook this installer writes runs this command with `=<agent>`;
/// entries containing it are the ones `install` replaces and `uninstall`
/// removes.
const HOOK_COMMAND_PREFIX: &str = "track prime --hook";
/// Seconds the hook may take. It reads a few small files and never touches
/// the network.
const HOOK_TIMEOUT_SECS: u64 = 10;
const OUTPUT_ENV: &str = "TRACK_OUTPUT";
const OUTPUT_ENV_VALUE: &str = "agent";

const BLOCK_START: &str = "<!-- track:begin";
const BLOCK_BEGIN: &str =
    "<!-- track:begin (managed by `track agent install`; edits here are overwritten) -->";
const BLOCK_END: &str = "<!-- track:end -->";

/// Project instructions, in AGENTS.md. Static on purpose: it names the
/// command that prints the facts, so it never goes stale.
const PROJECT_INSTRUCTIONS: &str = "\
## Issue tracker

This project's issues are managed with the `track` CLI.

- Before reading or changing issues, run `track prime`. It prints this project's state names, required fields, valid values and users. Use those; do not guess them.
- Add `-o agent` for compact output, or `-o json` to parse.
- A failed command prints `Code:` and often `Hint:`; follow the hint.
- `track guide` lists reference topics, such as this backend's query syntax.";

/// User-wide instructions: the same, for any project that uses track.
const USER_INSTRUCTIONS: &str = "\
## Issue tracker (track)

In a project with a `.track.toml`, issues are managed with the `track` CLI.

- Before reading or changing issues there, run `track prime`. It prints the project's state names, required fields, valid values and users. Use those; do not guess them.
- Add `-o agent` for compact output, or `-o json` to parse.
- A failed command prints `Code:` and often `Hint:`; follow the hint.
- `track guide` lists reference topics, such as the backend's query syntax.";

// ============================================================================
// Where each tool reads things
// ============================================================================

fn tool_label(tool: AgentTool) -> &'static str {
    match tool {
        AgentTool::Orek => "Orek.ai",
        AgentTool::Claude => "Claude Code",
        AgentTool::Codex => "Codex",
        AgentTool::Cursor => "Cursor",
        AgentTool::Antigravity => "Antigravity",
        AgentTool::Gemini => "Gemini CLI",
        AgentTool::Copilot => "Copilot",
    }
}

fn hook_agent(tool: AgentTool) -> Option<HookAgent> {
    match tool {
        AgentTool::Claude => Some(HookAgent::Claude),
        AgentTool::Codex => Some(HookAgent::Codex),
        AgentTool::Cursor => Some(HookAgent::Cursor),
        AgentTool::Antigravity => Some(HookAgent::Antigravity),
        AgentTool::Gemini => Some(HookAgent::Gemini),
        // Orek.ai has no session-start hook yet; Copilot's is not verified.
        AgentTool::Orek | AgentTool::Copilot => None,
    }
}

/// An environment variable, read the way `home_dir` reads them: empty
/// counts as unset.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The user's home directory. The environment wins over the platform lookup
/// so tests (and CI runners) can point it elsewhere.
pub fn home_dir() -> Option<PathBuf> {
    env_path("HOME")
        .or_else(|| env_path("USERPROFILE"))
        .or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            Some(PathBuf::from(format!(
                "{}{}",
                drive.to_string_lossy(),
                path.to_string_lossy()
            )))
        })
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
}

/// Codex's home: `$CODEX_HOME`, else `~/.codex`.
fn codex_home(home: &Path) -> PathBuf {
    env_path("CODEX_HOME").unwrap_or_else(|| home.join(".codex"))
}

/// Orek.ai's user configuration folder, per its configuration reference:
/// `$OREK_HOME/config`, else the platform's config location for `orek-ai`.
fn orek_user_config(home: &Path) -> PathBuf {
    orek_user_config_for(
        home,
        env_path("OREK_HOME"),
        env_path("XDG_CONFIG_HOME"),
        env_path("APPDATA"),
        std::env::consts::OS,
    )
}

fn orek_user_config_for(
    home: &Path,
    orek_home: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
    appdata: Option<PathBuf>,
    os: &str,
) -> PathBuf {
    if let Some(root) = orek_home {
        return root.join("config");
    }
    match os {
        "macos" => home.join("Library/Application Support/orek-ai"),
        "windows" => appdata
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
            .join("orek-ai")
            .join("config"),
        _ => xdg_config_home
            .unwrap_or_else(|| home.join(".config"))
            .join("orek-ai"),
    }
}

/// The folders a scope's paths are relative to.
struct Roots {
    scope: AgentScope,
    home: PathBuf,
    project: PathBuf,
}

impl Roots {
    fn new(scope: AgentScope) -> Result<Self> {
        Ok(Self {
            scope,
            home: home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?,
            project: std::env::current_dir().context("Cannot determine current directory")?,
        })
    }

    /// The folder a tool reads skills from in this scope, if it has one.
    fn skills_dir(&self, tool: AgentTool) -> Option<PathBuf> {
        let (home, project) = (&self.home, &self.project);
        match (self.scope, tool) {
            (AgentScope::User, AgentTool::Orek) => Some(orek_user_config(home).join("skills")),
            (AgentScope::User, AgentTool::Claude) => Some(home.join(".claude/skills")),
            (AgentScope::User, AgentTool::Codex | AgentTool::Cursor | AgentTool::Gemini) => {
                Some(home.join(".agents/skills"))
            }
            (AgentScope::User, AgentTool::Antigravity) => Some(home.join(".gemini/config/skills")),
            (AgentScope::User, AgentTool::Copilot) => Some(home.join(".copilot/skills")),
            (AgentScope::Project, AgentTool::Orek) => Some(project.join(".orek/skills")),
            (AgentScope::Project, AgentTool::Claude) => Some(project.join(".claude/skills")),
            (
                AgentScope::Project,
                AgentTool::Codex | AgentTool::Cursor | AgentTool::Gemini | AgentTool::Antigravity,
            ) => Some(project.join(".agents/skills")),
            (AgentScope::Project, AgentTool::Copilot) => None,
        }
    }

    /// The file a tool reads session-start hooks from in this scope.
    fn hook_file(&self, tool: AgentTool) -> Option<(PathBuf, HookStyle)> {
        let agent = hook_agent(tool)?;
        let (home, project) = (&self.home, &self.project);
        let user = self.scope == AgentScope::User;
        let path = match tool {
            AgentTool::Claude if user => home.join(".claude/settings.json"),
            AgentTool::Claude => project.join(".claude/settings.json"),
            AgentTool::Codex if user => codex_home(home).join("hooks.json"),
            AgentTool::Codex => project.join(".codex/hooks.json"),
            AgentTool::Cursor if user => home.join(".cursor/hooks.json"),
            AgentTool::Cursor => project.join(".cursor/hooks.json"),
            AgentTool::Antigravity if user => home.join(".gemini/config/hooks.json"),
            AgentTool::Antigravity => project.join(".agents/hooks.json"),
            AgentTool::Gemini if user => home.join(".gemini/settings.json"),
            AgentTool::Gemini => project.join(".gemini/settings.json"),
            AgentTool::Orek | AgentTool::Copilot => return None,
        };
        let style = match agent {
            // Same nesting: event, then matcher groups, then handlers.
            HookAgent::Claude => HookStyle::Nested {
                matcher: None,
                timeout: HOOK_TIMEOUT_SECS,
            },
            // Codex filters SessionStart on its `source`; these are the ones
            // a briefing belongs in.
            HookAgent::Codex => HookStyle::Nested {
                matcher: Some("startup|resume|clear|compact"),
                timeout: HOOK_TIMEOUT_SECS,
            },
            // Gemini CLI takes its timeout in milliseconds.
            HookAgent::Gemini => HookStyle::Nested {
                matcher: None,
                timeout: HOOK_TIMEOUT_SECS * 1000,
            },
            HookAgent::Cursor => HookStyle::Cursor,
            HookAgent::Antigravity => HookStyle::Antigravity,
            HookAgent::Orek => return None,
        };
        Some((path, style.with_command(agent)))
    }

    /// The config file that sets environment variables for the tool's
    /// shell commands, where one exists (Codex only; Claude Code's hook sets
    /// it itself and Cursor's hook returns it).
    fn env_file(&self, tool: AgentTool) -> Option<PathBuf> {
        match (tool, self.scope) {
            (AgentTool::Codex, AgentScope::User) => {
                Some(codex_home(&self.home).join("config.toml"))
            }
            (AgentTool::Codex, AgentScope::Project) => {
                Some(self.project.join(".codex/config.toml"))
            }
            _ => None,
        }
    }

    /// Files that carry the instructions block for this tool in this scope:
    /// `(path, block text, create when missing)`.
    fn instruction_files(&self, tool: AgentTool) -> Vec<(PathBuf, String, bool)> {
        let (home, project) = (&self.home, &self.project);
        match self.scope {
            AgentScope::User => match tool {
                AgentTool::Claude => vec![(
                    home.join(".claude/CLAUDE.md"),
                    USER_INSTRUCTIONS.to_string(),
                    true,
                )],
                AgentTool::Codex => {
                    // Codex reads AGENTS.override.md instead when it has content.
                    let dir = codex_home(home);
                    let over = dir.join("AGENTS.override.md");
                    let path = if std::fs::read_to_string(&over)
                        .is_ok_and(|text| !text.trim().is_empty())
                    {
                        over
                    } else {
                        dir.join("AGENTS.md")
                    };
                    vec![(path, USER_INSTRUCTIONS.to_string(), true)]
                }
                // One file serves both: Gemini CLI and Antigravity read it.
                AgentTool::Antigravity | AgentTool::Gemini => vec![(
                    home.join(".gemini/GEMINI.md"),
                    USER_INSTRUCTIONS.to_string(),
                    true,
                )],
                // Cursor's user rules and Orek.ai's have no file; Copilot's
                // are not verified.
                AgentTool::Cursor | AgentTool::Orek | AgentTool::Copilot => Vec::new(),
            },
            AgentScope::Project => {
                if tool == AgentTool::Copilot {
                    return Vec::new();
                }
                let mut files = vec![(
                    project.join("AGENTS.md"),
                    PROJECT_INSTRUCTIONS.to_string(),
                    true,
                )];
                // Claude Code skips AGENTS.md once a CLAUDE.md exists, and
                // Gemini CLI reads GEMINI.md unless configured otherwise:
                // an existing file of theirs gets an import of AGENTS.md.
                let imports: &[(&str, &str)] = match tool {
                    AgentTool::Claude => &[
                        ("CLAUDE.md", "@AGENTS.md"),
                        (".claude/CLAUDE.md", "@../AGENTS.md"),
                    ],
                    AgentTool::Gemini => &[("GEMINI.md", "@./AGENTS.md")],
                    _ => &[],
                };
                for (file, import) in imports {
                    files.push((
                        project.join(file),
                        format!("Issue-tracker instructions for this project are in AGENTS.md:\n\n{import}"),
                        false,
                    ));
                }
                files
            }
        }
    }

    /// Skill folders earlier versions wrote that no tool needs now. Removed
    /// so Cursor and Gemini CLI do not load the skill twice.
    fn legacy_skill_dirs(&self, tool: AgentTool) -> Vec<PathBuf> {
        match (self.scope, tool) {
            (AgentScope::User, AgentTool::Cursor) => vec![self.home.join(".cursor/skills/track")],
            (AgentScope::User, AgentTool::Gemini) => vec![self.home.join(".gemini/skills/track")],
            _ => Vec::new(),
        }
    }
}

fn selected(tools: &[AgentTool]) -> Vec<AgentTool> {
    if tools.is_empty() {
        AgentTool::ALL.to_vec()
    } else {
        AgentTool::ALL
            .iter()
            .copied()
            .filter(|tool| tools.contains(tool))
            .collect()
    }
}

// ============================================================================
// Skills
// ============================================================================

/// Write the skill tree into every folder the selected tools read in
/// `scope`. Returns each folder's tools and the path of its `SKILL.md`.
pub fn write_skills(scope: AgentScope, tools: &[AgentTool]) -> Result<Vec<(String, PathBuf)>> {
    let roots = Roots::new(scope)?;
    let mut by_dir: Vec<(PathBuf, Vec<&'static str>)> = Vec::new();
    for tool in selected(tools) {
        for legacy in roots.legacy_skill_dirs(tool) {
            remove_our_skill_dir(&legacy)?;
        }
        if let Some(dir) = roots.skills_dir(tool) {
            match by_dir.iter_mut().find(|(known, _)| *known == dir) {
                Some((_, labels)) => labels.push(tool_label(tool)),
                None => by_dir.push((dir, vec![tool_label(tool)])),
            }
        }
    }

    let mut installed = Vec::new();
    for (skills_dir, labels) in by_dir {
        let skill_dir = skills_dir.join("track");
        // References from an earlier version must not linger under names
        // the core no longer points to.
        let _ = std::fs::remove_dir_all(skill_dir.join("references"));
        for (relative, content) in SKILL_FILES {
            let path = skill_dir.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create {}", parent.display()))?;
            }
            std::fs::write(&path, content)
                .with_context(|| format!("Failed to write {}", path.display()))?;
        }
        installed.push((labels.join(", "), skill_dir.join("SKILL.md")));
    }
    Ok(installed)
}

/// Remove `dir` if it holds this skill (its SKILL.md is named `track`), and
/// leave anything else alone.
fn remove_our_skill_dir(dir: &Path) -> Result<bool> {
    let is_ours = std::fs::read_to_string(dir.join("SKILL.md")).is_ok_and(|text| {
        text.lines()
            .take(5)
            .any(|line| line.trim() == "name: track")
    });
    if !is_ours {
        return Ok(false);
    }
    std::fs::remove_dir_all(dir).with_context(|| format!("Failed to remove {}", dir.display()))?;
    Ok(true)
}

fn skill_state(dir: &Path) -> &'static str {
    let dir = dir.join("track");
    if !dir.join("SKILL.md").is_file() {
        "missing"
    } else if SKILL_FILES.iter().all(|(relative, content)| {
        std::fs::read_to_string(dir.join(relative)).is_ok_and(|text| text == *content)
    }) {
        "current"
    } else {
        "outdated"
    }
}

// ============================================================================
// Hook files (JSON)
// ============================================================================

/// How a tool's hook file nests a session-start command.
#[derive(Debug, Clone)]
enum HookStyle {
    /// `{"hooks": {"SessionStart": [{"matcher"?, "hooks": [handler]}]}}`:
    /// Claude Code, Codex, Gemini CLI.
    Nested {
        matcher: Option<&'static str>,
        timeout: u64,
    },
    /// `{"version": 1, "hooks": {"sessionStart": [handler]}}`: Cursor.
    Cursor,
    /// `{"<group>": {"SessionStart": [handler]}}`: Antigravity.
    Antigravity,
    /// A style bound to the command for one agent.
    Bound(Box<HookStyle>, String),
}

impl HookStyle {
    fn with_command(self, agent: HookAgent) -> Self {
        HookStyle::Bound(Box::new(self), format!("{HOOK_COMMAND_PREFIX}={agent}"))
    }
}

/// The group name this installer uses in Antigravity's hook file.
const ANTIGRAVITY_GROUP: &str = "track";

/// A JSON object whose key order and untouched values are kept as written.
type RawObject = IndexMap<String, Box<RawValue>>;

/// `value` as raw JSON, pretty-printed for a position `depth` levels into
/// the document, so what this installer writes reads like the rest of a
/// hand-edited settings file.
fn raw<T: Serialize + ?Sized>(value: &T, depth: usize) -> Result<Box<RawValue>> {
    let pretty = serde_json::to_string_pretty(value)?;
    let indent = "  ".repeat(depth);
    let text = pretty
        .lines()
        .enumerate()
        .map(|(n, line)| {
            if n == 0 {
                line.to_string()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(RawValue::from_string(text)?)
}

fn parse_object(value: Option<&RawValue>, what: &str) -> Result<RawObject> {
    match value {
        None => Ok(RawObject::new()),
        Some(value) => serde_json::from_str(value.get())
            .with_context(|| format!("`{what}` in the file is not an object")),
    }
}

fn parse_array(value: Option<&RawValue>, what: &str) -> Result<Vec<Box<RawValue>>> {
    match value {
        None => Ok(Vec::new()),
        Some(value) => serde_json::from_str(value.get())
            .with_context(|| format!("`{what}` in the file is not an array")),
    }
}

/// Whether a hook entry is one this installer wrote.
fn is_our_hook(entry: &RawValue) -> bool {
    entry.get().contains(HOOK_COMMAND_PREFIX)
}

fn put_or_remove(object: &mut RawObject, key: &str, value: Option<Box<RawValue>>) {
    match value {
        Some(value) => {
            object.insert(key.to_string(), value);
        }
        None => {
            object.shift_remove(key);
        }
    }
}

/// `text` with this installer's session-start hook present (`install`) or
/// absent. Everything else in the file is left as written.
fn edit_hook_file(text: &str, style: &HookStyle, install: bool) -> Result<String> {
    let HookStyle::Bound(style, command) = style else {
        unreachable!("hook styles are bound to a command before use")
    };
    let mut doc: RawObject = if text.trim().is_empty() {
        RawObject::new()
    } else {
        serde_json::from_str(text).context("the file is not a JSON object")?
    };

    match style.as_ref() {
        HookStyle::Nested { matcher, timeout } => {
            let mut hooks = parse_object(doc.get("hooks").map(|v| &**v), "hooks")?;
            let mut list = parse_array(
                hooks.get("SessionStart").map(|v| &**v),
                "hooks.SessionStart",
            )?;
            list.retain(|entry| !is_our_hook(entry));
            if install {
                let mut group = serde_json::Map::new();
                if let Some(matcher) = matcher {
                    group.insert("matcher".into(), (*matcher).into());
                }
                group.insert(
                    "hooks".into(),
                    serde_json::json!([{ "type": "command", "command": command, "timeout": timeout }]),
                );
                list.push(raw(&group, 3)?);
            }
            put_or_remove(
                &mut hooks,
                "SessionStart",
                (!list.is_empty()).then(|| raw(&list, 2)).transpose()?,
            );
            put_or_remove(
                &mut doc,
                "hooks",
                (!hooks.is_empty()).then(|| raw(&hooks, 1)).transpose()?,
            );
        }
        HookStyle::Cursor => {
            let mut hooks = parse_object(doc.get("hooks").map(|v| &**v), "hooks")?;
            let mut list = parse_array(
                hooks.get("sessionStart").map(|v| &**v),
                "hooks.sessionStart",
            )?;
            list.retain(|entry| !is_our_hook(entry));
            if install {
                list.push(raw(
                    &serde_json::json!({ "command": command, "timeout": HOOK_TIMEOUT_SECS }),
                    3,
                )?);
                if !doc.contains_key("version") {
                    doc.shift_insert(0, "version".to_string(), raw(&1, 1)?);
                }
            }
            put_or_remove(
                &mut hooks,
                "sessionStart",
                (!list.is_empty()).then(|| raw(&list, 2)).transpose()?,
            );
            put_or_remove(
                &mut doc,
                "hooks",
                (!hooks.is_empty()).then(|| raw(&hooks, 1)).transpose()?,
            );
            // A file left with nothing but its version holds no hooks.
            if doc.len() == 1 && doc.contains_key("version") {
                doc.clear();
            }
        }
        HookStyle::Antigravity => {
            if install {
                let group = serde_json::json!({
                    "SessionStart": [{ "type": "command", "command": command, "timeout": HOOK_TIMEOUT_SECS }]
                });
                doc.insert(ANTIGRAVITY_GROUP.to_string(), raw(&group, 1)?);
            } else if doc
                .get(ANTIGRAVITY_GROUP)
                .is_some_and(|group| is_our_hook(group))
            {
                doc.shift_remove(ANTIGRAVITY_GROUP);
            }
        }
        HookStyle::Bound(..) => unreachable!("hook styles are bound once"),
    }

    // Before 2.0 the Claude Code installer also set TRACK_OUTPUT in the
    // settings' `env`; the hook sets it now, so that entry goes.
    if !install || matches!(style.as_ref(), HookStyle::Nested { .. }) {
        remove_our_env_entry(&mut doc)?;
    }

    let mut out = serde_json::to_string_pretty(&doc)?;
    out.push('\n');
    Ok(out)
}

fn remove_our_env_entry(doc: &mut RawObject) -> Result<()> {
    let Some(env) = doc.get("env") else {
        return Ok(());
    };
    let Ok(mut env) = serde_json::from_str::<RawObject>(env.get()) else {
        return Ok(());
    };
    let ours = env
        .get(OUTPUT_ENV)
        .is_some_and(|value| value.get().trim() == format!("\"{OUTPUT_ENV_VALUE}\""));
    if ours {
        env.shift_remove(OUTPUT_ENV);
        put_or_remove(
            doc,
            "env",
            (!env.is_empty()).then(|| raw(&env, 1)).transpose()?,
        );
    }
    Ok(())
}

/// What a file edit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Written,
    Unchanged,
    Removed,
    Absent,
}

/// Apply `edit` to the file at `path`. A file that does not parse is left
/// alone and reported. A missing file is only created when installing, and
/// a file left empty (`{}`, no text) is removed.
fn update_file(
    path: &Path,
    install: bool,
    create: bool,
    edit: impl FnOnce(&str) -> Result<String>,
    is_empty: impl Fn(&str) -> bool,
) -> Result<Change> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err).with_context(|| format!("Failed to read {}", path.display())),
    };
    if existing.is_none() && !(install && create) {
        return Ok(Change::Absent);
    }
    let updated = edit(existing.as_deref().unwrap_or_default())
        .with_context(|| format!("{} was left unchanged", path.display()))?;
    if is_empty(&updated) {
        if existing.is_some() {
            std::fs::remove_file(path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
            return Ok(Change::Removed);
        }
        return Ok(Change::Absent);
    }
    if existing.as_deref() == Some(updated.as_str()) {
        return Ok(Change::Unchanged);
    }
    crate::fsutil::atomic_write(path, updated.as_bytes(), true)?;
    Ok(Change::Written)
}

fn is_empty_json(text: &str) -> bool {
    text.trim() == "{}"
}

// ============================================================================
// Codex config.toml
// ============================================================================

/// `text` with `[shell_environment_policy] set.TRACK_OUTPUT = "agent"`
/// present (`install`) or absent. A value the user chose is theirs to keep.
fn edit_codex_env(text: &str, install: bool) -> Result<String> {
    use toml_edit::{DocumentMut, Item, Table, value};
    let mut doc: DocumentMut = text.parse().context("the file is not valid TOML")?;
    if install {
        if !doc.contains_key("shell_environment_policy") {
            doc.insert("shell_environment_policy", Item::Table(Table::new()));
        }
        let policy = doc["shell_environment_policy"]
            .as_table_like_mut()
            .ok_or_else(|| anyhow!("`shell_environment_policy` is not a table"))?;
        if !policy.contains_key("set") {
            policy.insert("set", Item::Table(Table::new()));
        }
        let set = policy
            .get_mut("set")
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| anyhow!("`shell_environment_policy.set` is not a table"))?;
        if !set.contains_key(OUTPUT_ENV) {
            set.insert(OUTPUT_ENV, value(OUTPUT_ENV_VALUE));
        }
    } else if let Some(policy) = doc
        .get_mut("shell_environment_policy")
        .and_then(Item::as_table_like_mut)
    {
        if let Some(set) = policy.get_mut("set").and_then(Item::as_table_like_mut)
            && set.get(OUTPUT_ENV).and_then(Item::as_str) == Some(OUTPUT_ENV_VALUE)
        {
            set.remove(OUTPUT_ENV);
            if set.is_empty() {
                policy.remove("set");
            }
        }
        if policy.is_empty() {
            doc.remove("shell_environment_policy");
        }
    }
    Ok(doc.to_string())
}

fn codex_env_is_set(text: &str) -> bool {
    text.parse::<toml_edit::DocumentMut>().is_ok_and(|doc| {
        doc.get("shell_environment_policy")
            .and_then(|policy| policy.get("set"))
            .and_then(|set| set.get(OUTPUT_ENV))
            .is_some()
    })
}

// ============================================================================
// Instruction blocks (Markdown)
// ============================================================================

/// `text` with the managed block holding `content` (`Some`) or with no
/// managed block (`None`). Text outside the markers is never touched.
fn edit_block(text: &str, content: Option<&str>) -> String {
    let existing = text.find(BLOCK_START).and_then(|start| {
        text[start..]
            .find(BLOCK_END)
            .map(|end| (start, start + end + BLOCK_END.len()))
    });
    let block = content.map(|content| format!("{BLOCK_BEGIN}\n{content}\n{BLOCK_END}"));
    match (existing, block) {
        (Some((start, end)), Some(block)) => format!("{}{}{}", &text[..start], block, &text[end..]),
        (Some((start, end)), None) => {
            let before = text[..start].trim_end_matches('\n');
            let after = text[end..].trim_start_matches('\n');
            match (before.is_empty(), after.is_empty()) {
                (true, true) => String::new(),
                (true, false) => after.to_string(),
                (false, true) => format!("{before}\n"),
                (false, false) => format!("{before}\n\n{after}"),
            }
        }
        (None, Some(block)) if text.trim().is_empty() => format!("{block}\n"),
        (None, Some(block)) => format!("{}\n\n{block}\n", text.trim_end_matches('\n')),
        (None, None) => text.to_string(),
    }
}

// ============================================================================
// Commands
// ============================================================================

#[derive(Serialize, Default)]
struct ToolReport {
    tool: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    skill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_hook: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    instructions: Vec<String>,
}

#[derive(Serialize)]
struct InstallReport {
    success: bool,
    scope: &'static str,
    tools: Vec<ToolReport>,
    notes: Vec<String>,
}

fn scope_name(scope: AgentScope) -> &'static str {
    match scope {
        AgentScope::User => "user",
        AgentScope::Project => "project",
    }
}

pub fn handle_agent(action: &AgentCommands, format: OutputFormat) -> Result<()> {
    match action {
        AgentCommands::Install {
            scope,
            tools,
            skills_only,
            hooks_only,
        } => install(*scope, tools, !*hooks_only, !*skills_only, format),
        AgentCommands::Uninstall { scope, tools } => uninstall(*scope, tools, format),
        AgentCommands::Status => status(format),
    }
}

fn install(
    scope: AgentScope,
    tools: &[AgentTool],
    skills: bool,
    hooks: bool,
    format: OutputFormat,
) -> Result<()> {
    let roots = Roots::new(scope)?;
    let tools = selected(tools);
    let mut reports: Vec<ToolReport> = tools
        .iter()
        .map(|&tool| ToolReport {
            tool: tool_label(tool),
            ..Default::default()
        })
        .collect();
    let mut notes = Vec::new();

    if skills {
        let written = write_skills(scope, &tools)?;
        for (tool, report) in tools.iter().zip(reports.iter_mut()) {
            if let Some(dir) = roots.skills_dir(*tool) {
                let path = dir.join("track").join("SKILL.md");
                if written.iter().any(|(_, written)| *written == path) {
                    report.skill = Some(path.display().to_string());
                }
            }
        }
    }

    if hooks {
        for (&tool, report) in tools.iter().zip(reports.iter_mut()) {
            if let Some((path, style)) = roots.hook_file(tool) {
                update_file(
                    &path,
                    true,
                    true,
                    |text| edit_hook_file(text, &style, true),
                    is_empty_json,
                )?;
                report.session_hook = Some(path.display().to_string());
            }
            if let Some(path) = roots.env_file(tool) {
                update_file(
                    &path,
                    true,
                    true,
                    |text| edit_codex_env(text, true),
                    |t| t.trim().is_empty(),
                )?;
                report.environment = Some(path.display().to_string());
            }
            for (path, content, create) in roots.instruction_files(tool) {
                let change = update_file(
                    &path,
                    true,
                    create,
                    |text| Ok(edit_block(text, Some(&content))),
                    |t| t.trim().is_empty(),
                )?;
                if change != Change::Absent {
                    report.instructions.push(path.display().to_string());
                }
            }
        }
        if tools.contains(&AgentTool::Codex) {
            notes.push(
                "Codex runs a hook only after you trust it: open /hooks in Codex once and trust \"track prime --hook=codex\"."
                    .to_string(),
            );
        }
        if tools.contains(&AgentTool::Orek) {
            notes.push(if scope == AgentScope::Project {
                "Orek.ai has no session-start hook yet. It reads the skill and the AGENTS.md block; rerun `track agent install` once Orek.ai ships SessionStart hooks.".to_string()
            } else {
                "Orek.ai has no session-start hook or user instructions file yet, so only the skill is installed for you. Run `track agent install --scope project` in a repository to add the AGENTS.md block.".to_string()
            });
        }
        if tools.contains(&AgentTool::Copilot) {
            notes.push(
                "Copilot gets the skill only; its hook and instruction files are not verified."
                    .to_string(),
            );
        }
        notes.push(
            "Hooks print the briefing only in directories with a .track.toml. Run `track cache refresh` there once."
                .to_string(),
        );
        if scope == AgentScope::Project {
            notes.push("Commit the files listed above so the whole team gets them.".to_string());
        }
    }

    let report = InstallReport {
        success: true,
        scope: scope_name(scope),
        tools: reports,
        notes,
    };
    match format {
        OutputFormat::Json => output_json(&report)?,
        OutputFormat::Text => {
            use colored::Colorize;
            for tool in &report.tools {
                let nothing = tool.skill.is_none()
                    && tool.session_hook.is_none()
                    && tool.environment.is_none()
                    && tool.instructions.is_empty();
                if nothing {
                    continue;
                }
                println!("{}:", tool.tool.bold());
                if let Some(path) = &tool.skill {
                    println!("  {} skill: {path}", "Installed".green());
                }
                if let Some(path) = &tool.session_hook {
                    println!("  {} session hook: {path}", "Installed".green());
                }
                if let Some(path) = &tool.environment {
                    println!(
                        "  {} {OUTPUT_ENV}={OUTPUT_ENV_VALUE}: {path}",
                        "Set".green()
                    );
                }
                for path in &tool.instructions {
                    println!("  {} instructions: {path}", "Updated".green());
                }
            }
            for note in &report.notes {
                println!("{} {note}", "Note:".yellow());
            }
        }
    }
    Ok(())
}

fn uninstall(scope: AgentScope, tools: &[AgentTool], format: OutputFormat) -> Result<()> {
    let roots = Roots::new(scope)?;
    let tools = selected(tools);
    let mut removed: Vec<String> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut note = |path: &Path, what: &str| {
        let line = format!("{what} {}", path.display());
        if !removed.contains(&line) {
            removed.push(line);
        }
    };

    for &tool in &tools {
        let mut skill_dirs = roots.legacy_skill_dirs(tool);
        skill_dirs.extend(roots.skills_dir(tool).map(|dir| dir.join("track")));
        for dir in skill_dirs {
            if !seen.contains(&dir) && remove_our_skill_dir(&dir)? {
                note(&dir, "skill");
            }
            seen.push(dir);
        }
        if let Some((path, style)) = roots.hook_file(tool) {
            let change = update_file(
                &path,
                false,
                false,
                |text| edit_hook_file(text, &style, false),
                is_empty_json,
            )?;
            if matches!(change, Change::Written | Change::Removed) {
                note(&path, "session hook in");
            }
        }
        if let Some(path) = roots.env_file(tool) {
            let change = update_file(
                &path,
                false,
                false,
                |text| edit_codex_env(text, false),
                |t| t.trim().is_empty(),
            )?;
            if matches!(change, Change::Written | Change::Removed) {
                note(&path, "environment setting in");
            }
        }
        for (path, _, _) in roots.instruction_files(tool) {
            let change = update_file(
                &path,
                false,
                false,
                |text| Ok(edit_block(text, None)),
                |t| t.trim().is_empty(),
            )?;
            if matches!(change, Change::Written | Change::Removed) {
                note(&path, "instructions block in");
            }
        }
    }
    // Folders this installer may have created, removed only when empty.
    if scope == AgentScope::Project {
        for dir in [
            ".orek/skills",
            ".orek",
            ".claude/skills",
            ".claude",
            ".agents/skills",
            ".agents",
            ".codex",
            ".cursor",
            ".gemini",
        ] {
            let _ = std::fs::remove_dir(roots.project.join(dir));
        }
    }

    match format {
        OutputFormat::Json => output_json(&serde_json::json!({
            "success": true,
            "scope": scope_name(scope),
            "removed": removed,
        }))?,
        OutputFormat::Text => {
            if removed.is_empty() {
                println!("Nothing to remove for the {} scope.", scope_name(scope));
            }
            for item in &removed {
                println!("Removed {item}");
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct ToolStatus {
    tool: &'static str,
    /// `current`, `outdated` (written by another version), `missing`, or
    /// `not available` where the tool reads no skills in this scope
    skill: &'static str,
    /// `installed`, `missing`, or `not available` (no hook mechanism yet)
    session_hook: &'static str,
    /// `installed`, `missing`, or `not available`
    instructions: &'static str,
    /// `set`, `missing`, `from hook`, or `not available`
    environment: &'static str,
}

#[derive(Serialize)]
struct ScopeStatus {
    scope: &'static str,
    tools: Vec<ToolStatus>,
}

fn scope_status(scope: AgentScope) -> Result<ScopeStatus> {
    let roots = Roots::new(scope)?;
    let mark = |present: bool| if present { "installed" } else { "missing" };
    let tools = AgentTool::ALL
        .iter()
        .map(|&tool| {
            let skill = roots
                .skills_dir(tool)
                .map_or("not available", |dir| skill_state(&dir));
            let session_hook = roots.hook_file(tool).map_or("not available", |(path, _)| {
                mark(std::fs::read_to_string(path).is_ok_and(|text| {
                    text.contains(&format!(
                        "{HOOK_COMMAND_PREFIX}={}",
                        hook_agent(tool).map(|a| a.to_string()).unwrap_or_default()
                    ))
                }))
            });
            let files = roots.instruction_files(tool);
            let instructions = if files.is_empty() {
                "not available"
            } else {
                // The first file is the one the tool reads directly.
                mark(
                    std::fs::read_to_string(&files[0].0)
                        .is_ok_and(|text| text.contains(BLOCK_START)),
                )
            };
            let environment = match tool {
                AgentTool::Codex => roots.env_file(tool).map_or("missing", |path| {
                    if std::fs::read_to_string(path).is_ok_and(|text| codex_env_is_set(&text)) {
                        "set"
                    } else {
                        "missing"
                    }
                }),
                AgentTool::Claude | AgentTool::Cursor => "from hook",
                _ => "not available",
            };
            ToolStatus {
                tool: tool_label(tool),
                skill,
                session_hook,
                instructions,
                environment,
            }
        })
        .collect();
    Ok(ScopeStatus {
        scope: scope_name(scope),
        tools,
    })
}

fn status(format: OutputFormat) -> Result<()> {
    let scopes = vec![
        scope_status(AgentScope::User)?,
        scope_status(AgentScope::Project)?,
    ];
    match format {
        OutputFormat::Json => output_json(&serde_json::json!({ "scopes": scopes }))?,
        OutputFormat::Text => {
            for scope in &scopes {
                println!("{} scope:", scope.scope);
                println!(
                    "  {:<13} {:<14} {:<14} {:<14} environment",
                    "tool", "skill", "session hook", "instructions"
                );
                for tool in &scope.tools {
                    println!(
                        "  {:<13} {:<14} {:<14} {:<14} {}",
                        tool.tool,
                        tool.skill,
                        tool.session_hook,
                        tool.instructions,
                        tool.environment
                    );
                }
            }
        }
    }
    Ok(())
}

/// `track guide [topic]`: the references the skill points to, from the
/// binary itself, so they match the version in use and reach agents whose
/// tool does not load skills.
pub fn handle_guide(topic: Option<&str>, format: OutputFormat) -> Result<()> {
    let topics: Vec<(&str, &str)> = SKILL_FILES
        .iter()
        .filter_map(|(path, content)| {
            let name = path.strip_prefix("references/")?.strip_suffix(".md")?;
            Some((name, *content))
        })
        .collect();
    // The second line of each reference says when to read it.
    let purpose = |content: &str| {
        content
            .lines()
            .find(|line| line.starts_with("Read "))
            .unwrap_or_default()
            .to_string()
    };

    let Some(topic) = topic else {
        match format {
            OutputFormat::Json => output_json(
                &topics
                    .iter()
                    .map(|(name, content)| {
                        serde_json::json!({"topic": name, "read_when": purpose(content)})
                    })
                    .collect::<Vec<_>>(),
            )?,
            OutputFormat::Text => {
                println!("Topics (track guide <topic>):");
                for (name, content) in &topics {
                    println!("  {name}: {}", purpose(content));
                }
            }
        }
        return Ok(());
    };

    let wanted = topic.trim().to_lowercase();
    let Some((name, content)) = topics.iter().find(|(name, _)| *name == wanted) else {
        let names: Vec<&str> = topics.iter().map(|(name, _)| *name).collect();
        return Err(crate::errors::CliError::new(
            "unknown_topic",
            format!("No guide topic '{}'. Topics: {}", topic, names.join(", ")),
        )
        .into());
    };
    match format {
        OutputFormat::Json => output_json(&serde_json::json!({"topic": name, "text": content}))?,
        OutputFormat::Text => print!("{content}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_skill_stays_within_its_budget() {
        let (path, core) = SKILL_FILES[0];
        assert_eq!(path, "SKILL.md");
        assert!(
            core.len() <= CORE_SKILL_BUDGET_BYTES,
            "SKILL.md is {} bytes; move detail into a reference",
            core.len()
        );
    }

    #[test]
    fn every_reference_the_core_names_exists_and_says_when_to_read_it() {
        let core = SKILL_FILES[0].1;
        for (path, content) in &SKILL_FILES[1..] {
            let name = path
                .strip_prefix("references/")
                .and_then(|p| p.strip_suffix(".md"))
                .unwrap();
            assert!(
                core.contains(&format!("`{name}`")),
                "core does not name {name}"
            );
            assert!(content.starts_with("# "), "{path} needs a title");
            assert!(
                content.lines().any(|line| line.starts_with("Read ")),
                "{path} must say when to read it"
            );
        }
    }

    fn bound(tool: AgentTool, scope: AgentScope) -> HookStyle {
        let roots = Roots {
            scope,
            home: PathBuf::from("/home/u"),
            project: PathBuf::from("/work/p"),
        };
        roots.hook_file(tool).unwrap().1
    }

    #[test]
    fn claude_hooks_nest_under_session_start() {
        let out = edit_hook_file("", &bound(AgentTool::Claude, AgentScope::User), true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        let handler = &json["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(handler["command"], "track prime --hook=claude");
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["timeout"], 10);
        assert!(json["hooks"]["SessionStart"][0].get("matcher").is_none());
        // The hook sets the output format now; settings carry no env.
        assert!(json.get("env").is_none());
    }

    #[test]
    fn codex_and_gemini_use_the_same_nesting_with_their_own_details() {
        let codex = edit_hook_file("", &bound(AgentTool::Codex, AgentScope::User), true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&codex).unwrap();
        assert_eq!(
            json["hooks"]["SessionStart"][0]["matcher"],
            "startup|resume|clear|compact"
        );
        assert_eq!(
            json["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "track prime --hook=codex"
        );

        let gemini =
            edit_hook_file("", &bound(AgentTool::Gemini, AgentScope::Project), true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&gemini).unwrap();
        // Gemini CLI takes milliseconds.
        assert_eq!(
            json["hooks"]["SessionStart"][0]["hooks"][0]["timeout"],
            10_000
        );
    }

    #[test]
    fn cursor_hooks_are_flat_and_versioned() {
        let style = bound(AgentTool::Cursor, AgentScope::User);
        let out = edit_hook_file("", &style, true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(
            json["hooks"]["sessionStart"][0]["command"],
            "track prime --hook=cursor"
        );
        assert_eq!(edit_hook_file(&out, &style, false).unwrap(), "{}\n");

        // Someone else's hooks keep the file, and its version, alive.
        let theirs = r#"{"version": 1, "hooks": {"stop": [{"command": "say done"}]}}"#;
        let added = edit_hook_file(theirs, &style, true).unwrap();
        let removed = edit_hook_file(&added, &style, false).unwrap();
        let json: serde_json::Value = serde_json::from_str(&removed).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["hooks"]["stop"][0]["command"], "say done");
        assert!(json["hooks"].get("sessionStart").is_none());
    }

    #[test]
    fn antigravity_hooks_live_in_a_named_group() {
        let style = bound(AgentTool::Antigravity, AgentScope::Project);
        let theirs = r#"{"lint": {"PreToolUse": [{"type": "command", "command": "lint"}]}}"#;
        let out = edit_hook_file(theirs, &style, true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            json["track"]["SessionStart"][0]["command"],
            "track prime --hook=antigravity"
        );
        assert_eq!(json["track"]["SessionStart"][0]["timeout"], 10);
        assert_eq!(json["lint"]["PreToolUse"][0]["command"], "lint");
        let removed = edit_hook_file(&out, &style, false).unwrap();
        let json: serde_json::Value = serde_json::from_str(&removed).unwrap();
        assert!(json.get("track").is_none());
        assert!(json.get("lint").is_some());

        // A group of the user's that happens to be called "track" is theirs.
        let named = r#"{"track": {"Stop": [{"type": "command", "command": "echo"}]}}"#;
        assert!(
            edit_hook_file(named, &style, false)
                .unwrap()
                .contains("echo")
        );
    }

    #[test]
    fn install_keeps_key_order_and_everything_it_does_not_own() {
        let style = bound(AgentTool::Claude, AgentScope::User);
        let before = r#"{
  "zeta": {"b": 1,   "a": 2},
  "hooks": {
    "PreToolUse": [{"matcher": "Bash", "hooks": []}],
    "SessionStart": [{"hooks": [{"type": "command", "command": "echo hi"}]}]
  },
  "alpha": true,
  "env": {"OTHER": "1"}
}"#;
        let after = edit_hook_file(before, &style, true).unwrap();

        // Order of the user's keys, and their untouched text, survive.
        let position = |key: &str| after.find(key).unwrap();
        assert!(position("\"zeta\"") < position("\"hooks\""));
        assert!(position("\"hooks\"") < position("\"alpha\""));
        assert!(after.contains(r#"{"b": 1,   "a": 2}"#));
        assert!(after.contains("echo hi"));
        assert!(after.contains("PreToolUse"));
        let json: serde_json::Value = serde_json::from_str(&after).unwrap();
        assert_eq!(json["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
        assert_eq!(json["env"]["OTHER"], "1");

        // Installing again changes nothing.
        assert_eq!(edit_hook_file(&after, &style, true).unwrap(), after);

        // Uninstalling removes exactly what was added.
        let removed = edit_hook_file(&after, &style, false).unwrap();
        let json: serde_json::Value = serde_json::from_str(&removed).unwrap();
        assert_eq!(json["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert!(!removed.contains(HOOK_COMMAND_PREFIX));
        assert_eq!(json["env"]["OTHER"], "1");
    }

    #[test]
    fn a_hook_from_an_earlier_version_is_replaced_not_duplicated() {
        let style = bound(AgentTool::Claude, AgentScope::User);
        let old = r#"{"hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "track prime --hook", "timeout": 10}]}]}, "env": {"TRACK_OUTPUT": "agent"}}"#;
        let out = edit_hook_file(old, &style, true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        let list = json["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["hooks"][0]["command"], "track prime --hook=claude");
        // The 1.x installer's env entry goes; the hook sets it now.
        assert!(json.get("env").is_none());
    }

    #[test]
    fn a_user_chosen_output_format_is_not_removed() {
        let style = bound(AgentTool::Claude, AgentScope::User);
        let before = r#"{"env": {"TRACK_OUTPUT": "json"}}"#;
        let after = edit_hook_file(before, &style, true).unwrap();
        assert!(after.contains("\"json\""));
        assert!(
            edit_hook_file(&after, &style, false)
                .unwrap()
                .contains("\"json\"")
        );
    }

    #[test]
    fn hook_files_that_do_not_parse_are_refused() {
        let style = bound(AgentTool::Claude, AgentScope::User);
        assert!(edit_hook_file("{ not json", &style, true).is_err());
        assert!(edit_hook_file("[1, 2]", &style, true).is_err());
        assert!(edit_hook_file(r#"{"hooks": []}"#, &style, true).is_err());
    }

    #[test]
    fn codex_env_is_added_kept_and_removed() {
        let before = "model = \"x\"\n";
        let after = edit_codex_env(before, true).unwrap();
        assert!(after.starts_with("model = \"x\"\n"));
        assert!(codex_env_is_set(&after));
        let doc: toml::Value = toml::from_str(&after).unwrap();
        assert_eq!(
            doc["shell_environment_policy"]["set"]["TRACK_OUTPUT"].as_str(),
            Some("agent")
        );
        assert_eq!(edit_codex_env(&after, true).unwrap(), after);
        assert_eq!(edit_codex_env(&after, false).unwrap(), before);

        // A value the user set, and the user's other settings, stay.
        let theirs =
            "[shell_environment_policy]\ninherit = \"core\"\nset = { TRACK_OUTPUT = \"json\" }\n";
        assert_eq!(edit_codex_env(theirs, true).unwrap(), theirs);
        assert_eq!(edit_codex_env(theirs, false).unwrap(), theirs);
        assert!(edit_codex_env("not = [toml", true).is_err());
    }

    #[test]
    fn orek_user_config_follows_its_documented_locations() {
        let home = Path::new("/home/u");
        let at = |orek, xdg, appdata, os| orek_user_config_for(home, orek, xdg, appdata, os);
        assert_eq!(
            at(Some("/o".into()), None, None, "linux"),
            PathBuf::from("/o/config")
        );
        assert_eq!(
            at(None, None, None, "linux"),
            PathBuf::from("/home/u/.config/orek-ai")
        );
        assert_eq!(
            at(None, Some("/x".into()), None, "linux"),
            PathBuf::from("/x/orek-ai")
        );
        assert_eq!(
            at(None, None, None, "macos"),
            PathBuf::from("/home/u/Library/Application Support/orek-ai")
        );
        assert_eq!(
            at(None, None, Some("/r".into()), "windows"),
            PathBuf::from("/r/orek-ai/config")
        );
    }

    #[test]
    fn tools_that_share_a_folder_get_one_copy_of_the_skill() {
        let roots = Roots {
            scope: AgentScope::Project,
            home: PathBuf::from("/home/u"),
            project: PathBuf::from("/work/p"),
        };
        for tool in [
            AgentTool::Codex,
            AgentTool::Cursor,
            AgentTool::Gemini,
            AgentTool::Antigravity,
        ] {
            assert_eq!(
                roots.skills_dir(tool),
                Some(PathBuf::from("/work/p/.agents/skills"))
            );
        }
        assert_eq!(
            roots.skills_dir(AgentTool::Orek),
            Some(PathBuf::from("/work/p/.orek/skills"))
        );
        assert!(roots.hook_file(AgentTool::Orek).is_none());
    }

    #[test]
    fn the_instructions_block_is_added_replaced_and_removed_in_place() {
        let add = |text: &str| edit_block(text, Some(PROJECT_INSTRUCTIONS));
        let added = add("# Project\n\nNotes.\n");
        assert!(added.starts_with("# Project\n\nNotes.\n\n<!-- track:begin"));
        assert!(added.ends_with("<!-- track:end -->\n"));
        assert_eq!(add(&added), added);

        // An older block is replaced where it stands.
        let old = "# Project\n\n<!-- track:begin old -->\nstale\n<!-- track:end -->\n\n## After\n";
        let replaced = add(old);
        assert!(!replaced.contains("stale"));
        assert!(replaced.contains("track prime"));
        assert!(replaced.ends_with("\n\n## After\n"));

        assert_eq!(edit_block(&added, None), "# Project\n\nNotes.\n");
        assert_eq!(edit_block(&add(""), None), "");
        assert_eq!(edit_block(&replaced, None), "# Project\n\n## After\n");
    }
}
