//! Putting `track` in front of AI agents: the skill files, the session-start
//! hooks that print `track prime`, instruction blocks, and the embedded
//! reference guide.
//!
//! Every location below comes from the tool's own documentation (see
//! `docs/design/agent-integrations.md`). A tool without a documented
//! mechanism gets nothing for it rather than a guess.

use crate::cli::{AgentCommands, AgentScope, AgentTool, HookAgent, OutputFormat};
use crate::config::{env_path, home_dir};
use crate::fsutil;
use crate::output::{OUTPUT_ENV, OUTPUT_ENV_VALUE, output_json};
use anyhow::{Context, Result, anyhow};
use clap::ValueEnum;
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

/// The folder the skill is installed as, inside each tool's skills folder.
const SKILL_NAME: &str = "track";

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

/// `Codex, Cursor and Gemini CLI`
fn tool_list(tools: &[AgentTool]) -> String {
    let labels: Vec<&str> = tools.iter().map(|&tool| tool_label(tool)).collect();
    match labels.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// `Cursor and Gemini CLI also read it`
fn also_read_by(others: &[AgentTool]) -> String {
    let verb = if others.len() == 1 { "reads" } else { "read" };
    format!("{} also {verb} it", tool_list(others))
}

/// The command a session-start hook for `agent` runs.
fn hook_command(agent: HookAgent) -> String {
    format!("{HOOK_COMMAND_PREFIX}={agent}")
}

/// `base` joined with a relative path written with `/`, one component at a
/// time, so the path prints with the platform's separator. (A single
/// `join("a/b")` keeps the `/` on Windows and the shown path mixes both.)
fn under(base: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(base.to_path_buf(), |path, part| path.join(part))
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
        "macos" => under(home, "Library/Application Support/orek-ai"),
        "windows" => appdata
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
            .join("orek-ai")
            .join("config"),
        _ => xdg_config_home
            .unwrap_or_else(|| home.join(".config"))
            .join("orek-ai"),
    }
}

/// Where a tool reads its session-start hook, and what that hook runs.
struct HookTarget {
    path: PathBuf,
    style: HookStyle,
    command: String,
}

/// A file that carries the instructions block for a tool.
struct InstructionFile {
    path: PathBuf,
    content: String,
    /// Whether install creates the file when it is missing. Files that only
    /// import `AGENTS.md` are added to, never created.
    create: bool,
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

    /// The folder the skill is installed as for a tool in this scope, if
    /// the tool reads skills here.
    fn skill_dir(&self, tool: AgentTool) -> Option<PathBuf> {
        let (home, project) = (&self.home, &self.project);
        let skills = match (self.scope, tool) {
            (AgentScope::User, AgentTool::Orek) => orek_user_config(home).join("skills"),
            (AgentScope::User, AgentTool::Claude) => under(home, ".claude/skills"),
            (AgentScope::User, AgentTool::Codex | AgentTool::Cursor | AgentTool::Gemini) => {
                under(home, ".agents/skills")
            }
            (AgentScope::User, AgentTool::Antigravity) => under(home, ".gemini/config/skills"),
            (AgentScope::User, AgentTool::Copilot) => under(home, ".copilot/skills"),
            (AgentScope::Project, AgentTool::Orek) => under(project, ".orek/skills"),
            (AgentScope::Project, AgentTool::Claude) => under(project, ".claude/skills"),
            (
                AgentScope::Project,
                AgentTool::Codex | AgentTool::Cursor | AgentTool::Gemini | AgentTool::Antigravity,
            ) => under(project, ".agents/skills"),
            (AgentScope::Project, AgentTool::Copilot) => return None,
        };
        Some(skills.join(SKILL_NAME))
    }

    /// The file a tool reads session-start hooks from in this scope, the
    /// shape it nests them in, and the command ours runs.
    fn hook_target(&self, tool: AgentTool) -> Option<HookTarget> {
        let base = match self.scope {
            AgentScope::User => &self.home,
            AgentScope::Project => &self.project,
        };
        let user = self.scope == AgentScope::User;
        let (agent, path, style) = match tool {
            // Claude Code, Codex and Gemini CLI share one nesting: event,
            // then matcher groups, then handlers.
            AgentTool::Claude => (
                HookAgent::Claude,
                under(base, ".claude/settings.json"),
                HookStyle::Nested {
                    matcher: None,
                    timeout: HOOK_TIMEOUT_SECS,
                },
            ),
            AgentTool::Codex => (
                HookAgent::Codex,
                if user {
                    codex_home(&self.home).join("hooks.json")
                } else {
                    under(&self.project, ".codex/hooks.json")
                },
                // Codex filters SessionStart on its `source`; these are the
                // ones a briefing belongs in.
                HookStyle::Nested {
                    matcher: Some("startup|resume|clear|compact"),
                    timeout: HOOK_TIMEOUT_SECS,
                },
            ),
            AgentTool::Gemini => (
                HookAgent::Gemini,
                under(base, ".gemini/settings.json"),
                // Gemini CLI takes its timeout in milliseconds.
                HookStyle::Nested {
                    matcher: None,
                    timeout: HOOK_TIMEOUT_SECS * 1000,
                },
            ),
            AgentTool::Cursor => (
                HookAgent::Cursor,
                under(base, ".cursor/hooks.json"),
                HookStyle::Cursor,
            ),
            AgentTool::Antigravity => (
                HookAgent::Antigravity,
                if user {
                    under(&self.home, ".gemini/config/hooks.json")
                } else {
                    under(&self.project, ".agents/hooks.json")
                },
                HookStyle::Antigravity,
            ),
            // Orek.ai has no session-start hook yet; Copilot's is not verified.
            AgentTool::Orek | AgentTool::Copilot => return None,
        };
        Some(HookTarget {
            path,
            style,
            command: hook_command(agent),
        })
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
                Some(under(&self.project, ".codex/config.toml"))
            }
            _ => None,
        }
    }

    /// Files that carry the instructions block for this tool in this scope.
    fn instruction_files(&self, tool: AgentTool) -> Vec<InstructionFile> {
        let file = |path: PathBuf, content: &str, create: bool| InstructionFile {
            path,
            content: content.to_string(),
            create,
        };
        let (home, project) = (&self.home, &self.project);
        match self.scope {
            AgentScope::User => match tool {
                AgentTool::Claude => vec![file(
                    under(home, ".claude/CLAUDE.md"),
                    USER_INSTRUCTIONS,
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
                    vec![file(path, USER_INSTRUCTIONS, true)]
                }
                // One file serves both: Gemini CLI and Antigravity read it.
                AgentTool::Antigravity | AgentTool::Gemini => vec![file(
                    under(home, ".gemini/GEMINI.md"),
                    USER_INSTRUCTIONS,
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
                let mut files = vec![file(project.join("AGENTS.md"), PROJECT_INSTRUCTIONS, true)];
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
                for (name, import) in imports {
                    files.push(file(
                        under(project, name),
                        &format!(
                            "Issue-tracker instructions for this project are in AGENTS.md:\n\n{import}"
                        ),
                        false,
                    ));
                }
                files
            }
        }
    }

    /// Skill folders that 1.x (`track init --skills`) wrote and no tool
    /// needs now. Removed so Cursor and Gemini CLI do not load the skill
    /// twice.
    fn legacy_skill_dirs(&self, tool: AgentTool) -> Vec<PathBuf> {
        match (self.scope, tool) {
            (AgentScope::User, AgentTool::Cursor) => {
                vec![under(&self.home, ".cursor/skills/track")]
            }
            (AgentScope::User, AgentTool::Gemini) => {
                vec![under(&self.home, ".gemini/skills/track")]
            }
            _ => Vec::new(),
        }
    }

    /// The tools outside `selected` that also read what `reads` picks out:
    /// a folder or file several tools share stays while one of them is
    /// still set up.
    fn other_readers(
        &self,
        selected: &[AgentTool],
        reads: impl Fn(AgentTool) -> bool,
    ) -> Vec<AgentTool> {
        AgentTool::value_variants()
            .iter()
            .copied()
            .filter(|tool| !selected.contains(tool) && reads(*tool))
            .collect()
    }
}

fn selected(tools: &[AgentTool]) -> Vec<AgentTool> {
    AgentTool::value_variants()
        .iter()
        .copied()
        .filter(|tool| tools.is_empty() || tools.contains(tool))
        .collect()
}

/// Whether this installer's session-start hook for `tool` is in the file
/// the tool reads in `scope`. False where the tool has no hook, or the scope
/// cannot be resolved.
pub(crate) fn hook_installed(scope: AgentScope, tool: AgentTool) -> bool {
    Roots::new(scope)
        .ok()
        .and_then(|roots| hook_installed_in(&roots, tool))
        .unwrap_or(false)
}

/// `None` when the tool has no hook in this scope.
fn hook_installed_in(roots: &Roots, tool: AgentTool) -> Option<bool> {
    let target = roots.hook_target(tool)?;
    Some(std::fs::read_to_string(&target.path).is_ok_and(|text| text.contains(&target.command)))
}

// ============================================================================
// Skills
// ============================================================================

fn skill_md(skill_dir: &Path) -> PathBuf {
    skill_dir.join("SKILL.md")
}

/// One skill folder and the selected tools that read it.
struct SkillWrite {
    dir: PathBuf,
    tools: Vec<AgentTool>,
    result: Result<()>,
}

/// Write the skill tree once into every folder the given tools read. Each
/// folder succeeds or fails on its own.
fn write_skills_in(roots: &Roots, tools: &[AgentTool]) -> Vec<SkillWrite> {
    let mut writes: Vec<SkillWrite> = Vec::new();
    for &tool in tools {
        let Some(dir) = roots.skill_dir(tool) else {
            continue;
        };
        match writes.iter_mut().find(|write| write.dir == dir) {
            Some(write) => write.tools.push(tool),
            None => writes.push(SkillWrite {
                dir,
                tools: vec![tool],
                result: Ok(()),
            }),
        }
    }
    for write in &mut writes {
        write.result = write_skill_tree(&write.dir);
    }
    writes
}

fn write_skill_tree(skill_dir: &Path) -> Result<()> {
    // References from an earlier version must not linger under names the
    // core no longer points to.
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
    Ok(())
}

/// Remove the skill copies 1.x left where `tool` no longer reads them.
fn remove_legacy_skills(roots: &Roots, tool: AgentTool) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for dir in roots.legacy_skill_dirs(tool) {
        if remove_our_skill_dir(&dir)? {
            removed.push(dir);
        }
    }
    Ok(removed)
}

/// Write the skill tree into every folder the selected tools read in
/// `scope`. Returns one entry per folder: the tools that share it, and the
/// path of its `SKILL.md`.
pub fn write_skills(scope: AgentScope, tools: &[AgentTool]) -> Result<Vec<(String, PathBuf)>> {
    let roots = Roots::new(scope)?;
    let tools = selected(tools);
    for &tool in &tools {
        remove_legacy_skills(&roots, tool)?;
    }
    write_skills_in(&roots, &tools)
        .into_iter()
        .map(|write| {
            write.result?;
            let labels: Vec<&str> = write.tools.iter().map(|&tool| tool_label(tool)).collect();
            Ok((labels.join(", "), skill_md(&write.dir)))
        })
        .collect()
}

/// Whether `dir` holds this skill: its SKILL.md is named `track`.
fn is_our_skill_dir(dir: &Path) -> bool {
    std::fs::read_to_string(skill_md(dir)).is_ok_and(|text| {
        text.lines()
            .take(5)
            .any(|line| line.trim() == format!("name: {SKILL_NAME}"))
    })
}

/// Remove `dir` if it holds this skill, and leave anything else alone.
fn remove_our_skill_dir(dir: &Path) -> Result<bool> {
    if !is_our_skill_dir(dir) {
        return Ok(false);
    }
    std::fs::remove_dir_all(dir).with_context(|| format!("Failed to remove {}", dir.display()))?;
    Ok(true)
}

fn skill_state(skill_dir: &Path) -> &'static str {
    if !skill_md(skill_dir).is_file() {
        "missing"
    } else if SKILL_FILES.iter().all(|(relative, content)| {
        std::fs::read_to_string(skill_dir.join(relative)).is_ok_and(|text| text == *content)
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

/// Replace this installer's entries in `hooks.<event>` with `entry`, or
/// just drop them (`None`). A list or `hooks` object left empty goes too.
fn set_session_hook(
    doc: &mut RawObject,
    event: &str,
    entry: Option<serde_json::Value>,
) -> Result<()> {
    let mut hooks = parse_object(doc.get("hooks").map(|v| &**v), "hooks")?;
    let mut list = parse_array(hooks.get(event).map(|v| &**v), &format!("hooks.{event}"))?;
    list.retain(|entry| !is_our_hook(entry));
    if let Some(entry) = entry {
        list.push(raw(&entry, 3)?);
    }
    put_or_remove(
        &mut hooks,
        event,
        (!list.is_empty()).then(|| raw(&list, 2)).transpose()?,
    );
    put_or_remove(
        doc,
        "hooks",
        (!hooks.is_empty()).then(|| raw(&hooks, 1)).transpose()?,
    );
    Ok(())
}

/// Why a hook file cannot be edited: a rewrite would have to drop what the
/// parser cannot keep, so the file is refused instead.
fn unreadable_hook_file(text: &str, command: &str) -> String {
    let commented = text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("//") || line.starts_with("/*")
    });
    if commented {
        format!(
            "the file has comments, which rewriting it would lose; remove them, or add a \
             session-start hook running `{command}` by hand"
        )
    } else {
        "the file is not a JSON object".to_string()
    }
}

/// `text` with this installer's session-start hook running `command`
/// present (`install`) or absent. Everything else in the file is left as
/// written.
fn edit_hook_file(text: &str, style: &HookStyle, command: &str, install: bool) -> Result<String> {
    if !install && !text.contains(HOOK_COMMAND_PREFIX) {
        // Nothing of ours to take out, so nothing to parse: a file this
        // installer never edited is no reason for uninstall to fail.
        return Ok(text.to_string());
    }
    let mut doc: RawObject = if text.trim().is_empty() {
        RawObject::new()
    } else {
        serde_json::from_str(text).with_context(|| unreadable_hook_file(text, command))?
    };
    let handler = |timeout: u64| serde_json::json!({ "type": "command", "command": command, "timeout": timeout });

    match style {
        HookStyle::Nested { matcher, timeout } => {
            let entry = install.then(|| {
                let mut group = serde_json::Map::new();
                if let Some(matcher) = matcher {
                    group.insert("matcher".into(), (*matcher).into());
                }
                group.insert("hooks".into(), serde_json::json!([handler(*timeout)]));
                serde_json::Value::Object(group)
            });
            set_session_hook(&mut doc, "SessionStart", entry)?;
        }
        HookStyle::Cursor => {
            let entry = install
                .then(|| serde_json::json!({ "command": command, "timeout": HOOK_TIMEOUT_SECS }));
            set_session_hook(&mut doc, "sessionStart", entry)?;
            if install && !doc.contains_key("version") {
                doc.shift_insert(0, "version".to_string(), raw(&1, 1)?);
            }
            // A file left with nothing but its version holds no hooks.
            if doc.len() == 1 && doc.contains_key("version") {
                doc.clear();
            }
        }
        HookStyle::Antigravity => {
            if install {
                let group = serde_json::json!({ "SessionStart": [handler(HOOK_TIMEOUT_SECS)] });
                doc.insert(ANTIGRAVITY_GROUP.to_string(), raw(&group, 1)?);
            } else if doc
                .get(ANTIGRAVITY_GROUP)
                .is_some_and(|group| is_our_hook(group))
            {
                doc.shift_remove(ANTIGRAVITY_GROUP);
            }
        }
    }

    let mut out = serde_json::to_string_pretty(&doc)?;
    out.push('\n');
    Ok(out)
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
/// alone and reported. A missing file is only created when
/// `create_if_missing`. A file left empty (`{}`, no text) is removed, unless
/// it is a symlink: then the file it points at is emptied and the link kept.
fn update_file(
    path: &Path,
    create_if_missing: bool,
    edit: impl FnOnce(&str) -> Result<String>,
    is_empty: impl Fn(&str) -> bool,
) -> Result<Change> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err).with_context(|| format!("Failed to read {}", path.display())),
    };
    if existing.is_none() && !create_if_missing {
        return Ok(Change::Absent);
    }
    let updated = edit(existing.as_deref().unwrap_or_default())
        .with_context(|| format!("{} was left unchanged", path.display()))?;
    if existing.as_deref() == Some(updated.as_str()) {
        return Ok(Change::Unchanged);
    }
    if is_empty(&updated) {
        if existing.is_none() {
            return Ok(Change::Absent);
        }
        if !fsutil::is_symlink(path) {
            std::fs::remove_file(path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
            return Ok(Change::Removed);
        }
    }
    fsutil::atomic_write_user_file(path, updated.as_bytes())?;
    Ok(Change::Written)
}

fn is_empty_json(text: &str) -> bool {
    text.trim() == "{}"
}

fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}

// ============================================================================
// Codex config.toml
// ============================================================================

/// `text` with `[shell_environment_policy] set.TRACK_OUTPUT = "agent"`
/// present (`install`) or absent. A value the user chose is theirs to keep.
fn edit_codex_env(text: &str, install: bool) -> Result<String> {
    use toml_edit::{DocumentMut, Item, Table, value};
    if !install && !text.contains(OUTPUT_ENV) {
        return Ok(text.to_string());
    }
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
// One tool's files
// ============================================================================

/// The kinds of file a tool's setup touches besides its skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Artifact {
    Hook,
    Env,
    Instructions,
}

impl Artifact {
    /// How `uninstall` names what it took out of the file.
    fn removed_label(self) -> &'static str {
        match self {
            Artifact::Hook => "session hook in",
            Artifact::Env => "environment setting in",
            Artifact::Instructions => "instructions block in",
        }
    }
}

/// One file edit and how it went.
struct FileEdit {
    artifact: Artifact,
    path: PathBuf,
    change: Result<Change>,
}

/// Add (`install`) or remove a tool's session hook, environment setting and
/// instruction blocks. Each file is edited on its own, so one that cannot
/// be (a settings file with comments, say) is left as it was and reported
/// while the rest still go ahead. Files in `keep` are not touched.
fn sync_tool(roots: &Roots, tool: AgentTool, install: bool, keep: &[PathBuf]) -> Vec<FileEdit> {
    let mut edits = Vec::new();
    if let Some(target) = roots.hook_target(tool) {
        let change = update_file(
            &target.path,
            install,
            |text| edit_hook_file(text, &target.style, &target.command, install),
            is_empty_json,
        );
        edits.push(FileEdit {
            artifact: Artifact::Hook,
            path: target.path,
            change,
        });
    }
    if let Some(path) = roots.env_file(tool) {
        let change = update_file(
            &path,
            install,
            |text| edit_codex_env(text, install),
            is_blank,
        );
        edits.push(FileEdit {
            artifact: Artifact::Env,
            path,
            change,
        });
    }
    for file in roots.instruction_files(tool) {
        if keep.contains(&file.path) {
            continue;
        }
        let content = install.then_some(file.content.as_str());
        let change = update_file(
            &file.path,
            install && file.create,
            |text| Ok(edit_block(text, content)),
            is_blank,
        );
        edits.push(FileEdit {
            artifact: Artifact::Instructions,
            path: file.path,
            change,
        });
    }
    edits
}

/// The failure for commands that set up, or take down, several tools when
/// some of them did not get all the way.
fn not_every_tool(
    verb: &str,
    failed: &[(&'static str, Vec<String>)],
    total: usize,
) -> anyhow::Error {
    let details: Vec<String> = failed
        .iter()
        .map(|(tool, errors)| format!("{tool}: {}", errors.join("; ")))
        .collect();
    crate::errors::CliError::new(
        "partial_failure",
        format!(
            "{} of {total} tools not fully {verb}: {}",
            failed.len(),
            details.join(" | ")
        ),
    )
    .with_hint(
        "Everything else was done, and a file that did not parse was left exactly as it was. \
         Fix that file (or add the hook to it by hand), then run the command again.",
    )
    .into()
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
    /// What could not be set up for this tool, one entry per file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<String>,
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
    // Reports are in the order of `tools`.
    let report_of = |tool: AgentTool| {
        tools
            .iter()
            .position(|&known| known == tool)
            .expect("every selected tool has a report")
    };
    let mut notes = Vec::new();

    if skills {
        for &tool in &tools {
            if let Err(err) = remove_legacy_skills(&roots, tool) {
                reports[report_of(tool)].errors.push(format!("{err:#}"));
            }
        }
        for write in write_skills_in(&roots, &tools) {
            for &tool in &write.tools {
                let at = report_of(tool);
                match &write.result {
                    Ok(()) => reports[at].skill = Some(skill_md(&write.dir).display().to_string()),
                    Err(err) => reports[at].errors.push(format!("{err:#}")),
                }
            }
        }
    }

    if hooks {
        for (&tool, report) in tools.iter().zip(reports.iter_mut()) {
            for edit in sync_tool(&roots, tool, true, &[]) {
                let path = edit.path.display().to_string();
                match (edit.artifact, edit.change) {
                    (_, Err(err)) => report.errors.push(format!("{err:#}")),
                    (Artifact::Hook, Ok(_)) => report.session_hook = Some(path),
                    (Artifact::Env, Ok(_)) => report.environment = Some(path),
                    (Artifact::Instructions, Ok(Change::Absent)) => {}
                    (Artifact::Instructions, Ok(_)) => report.instructions.push(path),
                }
            }
        }
        if tools.contains(&AgentTool::Codex) {
            notes.push(format!(
                "Codex runs a hook only after you trust it: open /hooks in Codex once and trust \"{}\".",
                hook_command(HookAgent::Codex)
            ));
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

    let failed: Vec<(&'static str, Vec<String>)> = reports
        .iter()
        .filter(|report| !report.errors.is_empty())
        .map(|report| (report.tool, report.errors.clone()))
        .collect();
    let report = InstallReport {
        success: failed.is_empty(),
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
                    && tool.instructions.is_empty()
                    && tool.errors.is_empty();
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
                for error in &tool.errors {
                    println!("  {} {error}", "Not set up:".red());
                }
            }
            for note in &report.notes {
                println!("{} {note}", "Note:".yellow());
            }
        }
    }
    if !failed.is_empty() {
        return Err(not_every_tool("set up", &failed, report.tools.len()));
    }
    Ok(())
}

fn uninstall(scope: AgentScope, tools: &[AgentTool], format: OutputFormat) -> Result<()> {
    let roots = Roots::new(scope)?;
    let tools = selected(tools);
    let mut removed: Vec<String> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    let mut failed: Vec<(&'static str, Vec<String>)> = Vec::new();
    let mut fail = |tool: AgentTool, err: anyhow::Error| {
        let label = tool_label(tool);
        match failed.iter_mut().find(|(known, _)| *known == label) {
            Some((_, errors)) => errors.push(format!("{err:#}")),
            None => failed.push((label, vec![format!("{err:#}")])),
        }
    };
    let mut note = |line: String| {
        if !removed.contains(&line) {
            removed.push(line);
        }
    };
    // Every path visited, for pruning the folders install created.
    let mut visited: Vec<PathBuf> = Vec::new();

    // Skills. One folder can serve several tools: it goes only once none of
    // the tools that read it remain.
    let mut skill_dirs: Vec<(PathBuf, AgentTool)> = Vec::new();
    for &tool in &tools {
        match remove_legacy_skills(&roots, tool) {
            Ok(dirs) => dirs
                .iter()
                .for_each(|dir| note(format!("skill {}", dir.display()))),
            Err(err) => fail(tool, err),
        }
        if let Some(dir) = roots.skill_dir(tool)
            && !skill_dirs.iter().any(|(known, _)| *known == dir)
        {
            skill_dirs.push((dir, tool));
        }
    }
    for (dir, tool) in skill_dirs {
        let others = roots.other_readers(&tools, |other| {
            roots.skill_dir(other).as_ref() == Some(&dir)
        });
        if !others.is_empty() {
            if is_our_skill_dir(&dir) {
                kept.push(format!(
                    "skill {} ({})",
                    dir.display(),
                    also_read_by(&others)
                ));
            }
            continue;
        }
        match remove_our_skill_dir(&dir) {
            Ok(true) => note(format!("skill {}", dir.display())),
            Ok(false) => {}
            Err(err) => fail(tool, err),
        }
        visited.push(dir);
    }

    // Instruction files several tools read (AGENTS.md, GEMINI.md) keep their
    // block while one of those tools remains.
    let mut keep: Vec<PathBuf> = Vec::new();
    for &tool in &tools {
        for file in roots.instruction_files(tool) {
            if keep.contains(&file.path) {
                continue;
            }
            let others = roots.other_readers(&tools, |other| {
                roots
                    .instruction_files(other)
                    .iter()
                    .any(|theirs| theirs.path == file.path)
            });
            if others.is_empty() {
                continue;
            }
            if std::fs::read_to_string(&file.path).is_ok_and(|text| text.contains(BLOCK_START)) {
                kept.push(format!(
                    "instructions block in {} ({})",
                    file.path.display(),
                    also_read_by(&others)
                ));
            }
            keep.push(file.path);
        }
    }

    for &tool in &tools {
        for edit in sync_tool(&roots, tool, false, &keep) {
            match edit.change {
                Ok(Change::Written | Change::Removed) => note(format!(
                    "{} {}",
                    edit.artifact.removed_label(),
                    edit.path.display()
                )),
                Ok(Change::Unchanged | Change::Absent) => {}
                Err(err) => fail(tool, err),
            }
            visited.push(edit.path);
        }
    }

    // Folders install may have created, removed only when empty.
    if scope == AgentScope::Project {
        prune_empty_folders(&roots.project, &visited);
    }

    match format {
        OutputFormat::Json => output_json(&serde_json::json!({
            "success": failed.is_empty(),
            "scope": scope_name(scope),
            "removed": removed,
            "kept": kept,
        }))?,
        OutputFormat::Text => {
            if removed.is_empty() && kept.is_empty() {
                println!("Nothing to remove for the {} scope.", scope_name(scope));
            }
            for item in &removed {
                println!("Removed {item}");
            }
            for item in &kept {
                println!("Kept {item}");
            }
            if !kept.is_empty() {
                println!(
                    "To remove what was kept, name those tools in --tools too, or run `track agent uninstall{}` without --tools.",
                    if scope == AgentScope::Project {
                        " --scope project"
                    } else {
                        ""
                    }
                );
            }
        }
    }
    if !failed.is_empty() {
        return Err(not_every_tool("removed", &failed, tools.len()));
    }
    Ok(())
}

/// Remove the folder holding each of `paths`, then its parents, while they
/// are empty and inside `root` (never `root` itself). A folder that is not
/// empty ends that walk; a deeper path walked later still clears it.
fn prune_empty_folders(root: &Path, paths: &[PathBuf]) {
    for path in paths {
        let mut folder = path.parent();
        while let Some(dir) = folder {
            if dir == root || !dir.starts_with(root) || std::fs::remove_dir(dir).is_err() {
                break;
            }
            folder = dir.parent();
        }
    }
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
    let tools = AgentTool::value_variants()
        .iter()
        .map(|&tool| {
            let skill = roots
                .skill_dir(tool)
                .map_or("not available", |dir| skill_state(&dir));
            let session_hook = hook_installed_in(&roots, tool).map_or("not available", mark);
            let files = roots.instruction_files(tool);
            let instructions = match files.first() {
                None => "not available",
                // The first file is the one the tool reads directly.
                Some(file) => mark(
                    std::fs::read_to_string(&file.path)
                        .is_ok_and(|text| text.contains(BLOCK_START)),
                ),
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
    fn relative_paths_are_joined_one_component_at_a_time() {
        let base = Path::new("home");
        assert_eq!(
            under(base, ".agents/skills"),
            base.join(".agents").join("skills")
        );
        assert_eq!(under(base, "AGENTS.md"), base.join("AGENTS.md"));
    }

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

    fn roots(scope: AgentScope) -> Roots {
        Roots {
            scope,
            home: PathBuf::from("/home/u"),
            project: PathBuf::from("/work/p"),
        }
    }

    /// `text` with the hook for `tool` in `scope` added or removed.
    fn edit(text: &str, tool: AgentTool, scope: AgentScope, install: bool) -> Result<String> {
        let target = roots(scope).hook_target(tool).unwrap();
        edit_hook_file(text, &target.style, &target.command, install)
    }

    #[test]
    fn the_hook_command_names_the_agent_as_clap_spells_it() {
        for agent in HookAgent::value_variants() {
            let name = agent.to_possible_value().unwrap().get_name().to_string();
            assert_eq!(hook_command(*agent), format!("track prime --hook={name}"));
        }
        assert_eq!(
            roots(AgentScope::User)
                .hook_target(AgentTool::Gemini)
                .unwrap()
                .command,
            "track prime --hook=gemini"
        );
        // The value the installers set is the one `-o` reads.
        assert!(matches!(
            <crate::cli::CliFormat as ValueEnum>::from_str(OUTPUT_ENV_VALUE, true),
            Ok(crate::cli::CliFormat::Agent)
        ));
    }

    #[test]
    fn claude_hooks_nest_under_session_start() {
        let out = edit("", AgentTool::Claude, AgentScope::User, true).unwrap();
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
        let codex = edit("", AgentTool::Codex, AgentScope::User, true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&codex).unwrap();
        assert_eq!(
            json["hooks"]["SessionStart"][0]["matcher"],
            "startup|resume|clear|compact"
        );
        assert_eq!(
            json["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "track prime --hook=codex"
        );

        let gemini = edit("", AgentTool::Gemini, AgentScope::Project, true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&gemini).unwrap();
        // Gemini CLI takes milliseconds.
        assert_eq!(
            json["hooks"]["SessionStart"][0]["hooks"][0]["timeout"],
            10_000
        );
    }

    #[test]
    fn cursor_hooks_are_flat_and_versioned() {
        let t = roots(AgentScope::User)
            .hook_target(AgentTool::Cursor)
            .unwrap();
        let out = edit_hook_file("", &t.style, &t.command, true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(
            json["hooks"]["sessionStart"][0]["command"],
            "track prime --hook=cursor"
        );
        assert_eq!(
            edit_hook_file(&out, &t.style, &t.command, false).unwrap(),
            "{}\n"
        );

        // Someone else's hooks keep the file, and its version, alive.
        let theirs = r#"{"version": 1, "hooks": {"stop": [{"command": "say done"}]}}"#;
        let added = edit_hook_file(theirs, &t.style, &t.command, true).unwrap();
        let removed = edit_hook_file(&added, &t.style, &t.command, false).unwrap();
        let json: serde_json::Value = serde_json::from_str(&removed).unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["hooks"]["stop"][0]["command"], "say done");
        assert!(json["hooks"].get("sessionStart").is_none());
    }

    #[test]
    fn antigravity_hooks_live_in_a_named_group() {
        let t = roots(AgentScope::Project)
            .hook_target(AgentTool::Antigravity)
            .unwrap();
        let theirs = r#"{"lint": {"PreToolUse": [{"type": "command", "command": "lint"}]}}"#;
        let out = edit_hook_file(theirs, &t.style, &t.command, true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            json["track"]["SessionStart"][0]["command"],
            "track prime --hook=antigravity"
        );
        assert_eq!(json["track"]["SessionStart"][0]["timeout"], 10);
        assert_eq!(json["lint"]["PreToolUse"][0]["command"], "lint");
        let removed = edit_hook_file(&out, &t.style, &t.command, false).unwrap();
        let json: serde_json::Value = serde_json::from_str(&removed).unwrap();
        assert!(json.get("track").is_none());
        assert!(json.get("lint").is_some());

        // A group of the user's that happens to be called "track" is theirs.
        let named = r#"{"track": {"Stop": [{"type": "command", "command": "echo"}]}}"#;
        assert!(
            edit_hook_file(named, &t.style, &t.command, false)
                .unwrap()
                .contains("echo")
        );
    }

    #[test]
    fn install_keeps_key_order_and_everything_it_does_not_own() {
        let t = roots(AgentScope::User)
            .hook_target(AgentTool::Claude)
            .unwrap();
        let before = r#"{
  "zeta": {"b": 1,   "a": 2},
  "hooks": {
    "PreToolUse": [{"matcher": "Bash", "hooks": []}],
    "SessionStart": [{"hooks": [{"type": "command", "command": "echo hi"}]}]
  },
  "alpha": true,
  "env": {"OTHER": "1"}
}"#;
        let after = edit_hook_file(before, &t.style, &t.command, true).unwrap();

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
        assert_eq!(
            edit_hook_file(&after, &t.style, &t.command, true).unwrap(),
            after
        );

        // Uninstalling removes exactly what was added.
        let removed = edit_hook_file(&after, &t.style, &t.command, false).unwrap();
        let json: serde_json::Value = serde_json::from_str(&removed).unwrap();
        assert_eq!(json["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert!(!removed.contains(HOOK_COMMAND_PREFIX));
        assert_eq!(json["env"]["OTHER"], "1");
    }

    #[test]
    fn a_hook_of_ours_with_another_command_is_replaced_not_duplicated() {
        // A hand-written entry, or one from a build that ran plain
        // `track prime --hook`, matches by the command prefix.
        let t = roots(AgentScope::User)
            .hook_target(AgentTool::Claude)
            .unwrap();
        let old = r#"{"hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "track prime --hook", "timeout": 10}]}]}, "env": {"TRACK_OUTPUT": "agent"}}"#;
        let out = edit_hook_file(old, &t.style, &t.command, true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        let list = json["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["hooks"][0]["command"], "track prime --hook=claude");
        // `env` is the user's: the installer never wrote it, so it stays.
        assert_eq!(json["env"]["TRACK_OUTPUT"], "agent");
        let removed = edit_hook_file(&out, &t.style, &t.command, false).unwrap();
        assert!(removed.contains("\"TRACK_OUTPUT\": \"agent\""), "{removed}");
    }

    #[test]
    fn hook_files_that_do_not_parse_are_refused() {
        let t = roots(AgentScope::User)
            .hook_target(AgentTool::Claude)
            .unwrap();
        assert!(edit_hook_file("{ not json", &t.style, &t.command, true).is_err());
        assert!(edit_hook_file("[1, 2]", &t.style, &t.command, true).is_err());
        assert!(edit_hook_file(r#"{"hooks": []}"#, &t.style, &t.command, true).is_err());

        // Gemini CLI's settings allow comments, which a rewrite would drop.
        let t = roots(AgentScope::User)
            .hook_target(AgentTool::Gemini)
            .unwrap();
        let commented = "{\n  // my theme\n  \"theme\": \"dark\"\n}\n";
        let err = edit_hook_file(commented, &t.style, &t.command, true).unwrap_err();
        assert!(format!("{err:#}").contains("comments"), "{err:#}");
        assert!(
            format!("{err:#}").contains("track prime --hook=gemini"),
            "{err:#}"
        );
        // Uninstall has nothing to take out of it, so it does not refuse it.
        assert_eq!(
            edit_hook_file(commented, &t.style, &t.command, false).unwrap(),
            commented
        );
        assert_eq!(edit_codex_env("not = [toml", false).unwrap(), "not = [toml");
    }

    #[test]
    fn a_file_that_cannot_be_edited_is_left_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let commented = "{\n  // my theme\n  \"theme\": \"dark\"\n}\n";
        std::fs::write(&path, commented).unwrap();
        let t = roots(AgentScope::User)
            .hook_target(AgentTool::Gemini)
            .unwrap();

        let err = update_file(
            &path,
            true,
            |text| edit_hook_file(text, &t.style, &t.command, true),
            is_empty_json,
        )
        .unwrap_err();

        assert!(format!("{err:#}").contains("was left unchanged"), "{err:#}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), commented);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_left_empty_keeps_its_link() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dotfiles-settings.json");
        let t = roots(AgentScope::User)
            .hook_target(AgentTool::Claude)
            .unwrap();
        std::fs::write(
            &target,
            edit_hook_file("", &t.style, &t.command, true).unwrap(),
        )
        .unwrap();
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let change = update_file(
            &link,
            false,
            |text| edit_hook_file(text, &t.style, &t.command, false),
            is_empty_json,
        )
        .unwrap();

        assert_eq!(change, Change::Written);
        assert!(fsutil::is_symlink(&link));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{}\n");
    }

    #[test]
    fn pruning_removes_empty_folders_up_to_the_project_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(root.join(".agents/skills")).unwrap();
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::write(root.join(".claude/keep.txt"), "mine").unwrap();

        prune_empty_folders(
            &root,
            &[
                root.join(".claude/settings.json"),
                root.join(".agents/skills/track"),
                root.join("AGENTS.md"),
            ],
        );

        assert!(!root.join(".agents").exists());
        assert!(root.join(".claude/keep.txt").exists());
        assert!(root.is_dir());
    }

    #[test]
    fn shared_files_name_the_other_tools_that_read_them() {
        let user = roots(AgentScope::User);
        let skills = user.skill_dir(AgentTool::Codex).unwrap();
        let others = user.other_readers(&[AgentTool::Codex], |tool| {
            user.skill_dir(tool).as_ref() == Some(&skills)
        });
        assert_eq!(others, [AgentTool::Cursor, AgentTool::Gemini]);
        assert_eq!(also_read_by(&others), "Cursor and Gemini CLI also read it");
        assert_eq!(also_read_by(&others[..1]), "Cursor also reads it");

        let project = roots(AgentScope::Project);
        let agents_md = PathBuf::from("/work/p/AGENTS.md");
        let reads = |tool| {
            project
                .instruction_files(tool)
                .iter()
                .any(|file| file.path == agents_md)
        };
        let everyone_else: Vec<AgentTool> = AgentTool::value_variants()
            .iter()
            .copied()
            .filter(|&tool| tool != AgentTool::Cursor && tool != AgentTool::Copilot)
            .collect();
        assert_eq!(
            project.other_readers(&[AgentTool::Cursor], reads),
            everyone_else
        );
        assert!(
            project
                .other_readers(AgentTool::value_variants(), reads)
                .is_empty()
        );
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
                roots.skill_dir(tool),
                Some(PathBuf::from("/work/p/.agents/skills/track"))
            );
        }
        assert_eq!(
            roots.skill_dir(AgentTool::Orek),
            Some(PathBuf::from("/work/p/.orek/skills/track"))
        );
        assert!(roots.hook_target(AgentTool::Orek).is_none());
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
