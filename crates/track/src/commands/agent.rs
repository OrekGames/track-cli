//! Putting `track` in front of AI agents: the skill files, the session hook
//! that prints `track prime`, and the embedded reference guide.

use crate::cli::{AgentCommands, AgentScope, OutputFormat};
use crate::output::output_json;
use anyhow::{Context, Result, anyhow};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::value::RawValue;
use std::path::PathBuf;

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
/// used, while references are read one at a time.
#[cfg(test)]
const CORE_SKILL_BUDGET_BYTES: usize = 6 * 1024;

/// AI coding tools that read `~/.<dir>/skills/<name>/SKILL.md`.
const SKILL_TOOL_DIRS: &[(&str, &str)] = &[
    ("Claude Code", ".claude"),
    ("Copilot", ".copilot"),
    ("Cursor", ".cursor"),
    ("Gemini CLI", ".gemini"),
];

/// The command the session hook runs.
const HOOK_COMMAND: &str = "track prime --hook";
/// Seconds the hook may take. It reads a few small files and never touches
/// the network.
const HOOK_TIMEOUT_SECS: u32 = 10;
const OUTPUT_ENV: &str = "TRACK_OUTPUT";
const OUTPUT_ENV_VALUE: &str = "agent";

const BLOCK_BEGIN: &str =
    "<!-- track:begin (managed by `track agent install`; edits here are overwritten) -->";
const BLOCK_END: &str = "<!-- track:end -->";
/// Static on purpose: it names the command that prints the facts, so it
/// never goes stale and never churns the repository.
const INSTRUCTIONS: &str = "\
## Issue tracker

This project's issues are managed with the `track` CLI.

- Before reading or changing issues, run `track prime`. It prints this project's state names, required fields, valid values and users. Use those; do not guess them.
- Add `-o agent` for compact output, or `-o json` to parse.
- A failed command prints `Code:` and often `Hint:`; follow the hint.
- `track guide` lists reference topics, such as this backend's query syntax.";

/// The user's home directory. The environment wins over the platform lookup
/// so tests (and CI runners) can point it elsewhere.
pub fn home_dir() -> Option<PathBuf> {
    let from_env = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    from_env("HOME")
        .or_else(|| from_env("USERPROFILE"))
        .map(PathBuf::from)
        .or_else(|| {
            let drive = from_env("HOMEDRIVE")?;
            let path = from_env("HOMEPATH")?;
            Some(PathBuf::from(format!(
                "{}{}",
                drive.to_string_lossy(),
                path.to_string_lossy()
            )))
        })
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
}

fn scope_root(scope: AgentScope) -> Result<PathBuf> {
    match scope {
        AgentScope::User => home_dir().ok_or_else(|| anyhow!("Cannot determine home directory")),
        AgentScope::Project => {
            std::env::current_dir().context("Cannot determine current directory")
        }
    }
}

/// Tools whose skills are installed under `root` for `scope`. In a project
/// only Claude Code is known to read project-level skills.
fn skill_tools(scope: AgentScope) -> &'static [(&'static str, &'static str)] {
    match scope {
        AgentScope::User => SKILL_TOOL_DIRS,
        AgentScope::Project => &SKILL_TOOL_DIRS[..1],
    }
}

/// Write the skill tree for every tool of `scope` and return each tool with
/// the path of its `SKILL.md`.
pub fn write_skills(scope: AgentScope) -> Result<Vec<(&'static str, PathBuf)>> {
    let root = scope_root(scope)?;
    let mut installed = Vec::new();
    for &(tool, tool_dir) in skill_tools(scope) {
        let skill_dir = root.join(tool_dir).join("skills").join("track");
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
        installed.push((tool, skill_dir.join("SKILL.md")));
    }
    Ok(installed)
}

fn remove_skills(scope: AgentScope) -> Result<Vec<PathBuf>> {
    let root = scope_root(scope)?;
    let mut removed = Vec::new();
    for &(_, tool_dir) in skill_tools(scope) {
        let skill_dir = root.join(tool_dir).join("skills").join("track");
        if skill_dir.is_dir() {
            std::fs::remove_dir_all(&skill_dir)
                .with_context(|| format!("Failed to remove {}", skill_dir.display()))?;
            removed.push(skill_dir);
        }
    }
    Ok(removed)
}

// ============================================================================
// Claude Code settings.json
// ============================================================================

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
            .with_context(|| format!("`{what}` in the settings file is not an object")),
    }
}

/// Whether a SessionStart entry is the one this installer wrote.
fn is_our_hook(entry: &RawValue) -> bool {
    entry.get().contains(HOOK_COMMAND)
}

/// `settings` with the session hook and the output variable present
/// (`install`) or absent (`!install`). Everything else is left as written.
fn edit_settings(text: &str, install: bool) -> Result<String> {
    let mut settings: RawObject = if text.trim().is_empty() {
        RawObject::new()
    } else {
        serde_json::from_str(text).context("the settings file is not a JSON object")?
    };

    // hooks.SessionStart
    let mut hooks = parse_object(settings.get("hooks").map(|v| &**v), "hooks")?;
    let mut session_start: Vec<Box<RawValue>> = match hooks.get("SessionStart") {
        None => Vec::new(),
        Some(value) => serde_json::from_str(value.get())
            .context("`hooks.SessionStart` in the settings file is not an array")?,
    };
    session_start.retain(|entry| !is_our_hook(entry));
    if install {
        session_start.push(raw(
            &serde_json::json!({
                "hooks": [{
                    "type": "command",
                    "command": HOOK_COMMAND,
                    "timeout": HOOK_TIMEOUT_SECS,
                }]
            }),
            3,
        )?);
    }
    if session_start.is_empty() {
        hooks.shift_remove("SessionStart");
    } else {
        hooks.insert("SessionStart".to_string(), raw(&session_start, 2)?);
    }
    if hooks.is_empty() {
        settings.shift_remove("hooks");
    } else {
        settings.insert("hooks".to_string(), raw(&hooks, 1)?);
    }

    // env.TRACK_OUTPUT
    let mut env = parse_object(settings.get("env").map(|v| &**v), "env")?;
    let ours = env
        .get(OUTPUT_ENV)
        .is_some_and(|value| value.get().trim() == format!("\"{OUTPUT_ENV_VALUE}\""));
    if install {
        // A value the user chose is theirs to keep.
        if !env.contains_key(OUTPUT_ENV) {
            env.insert(OUTPUT_ENV.to_string(), raw(OUTPUT_ENV_VALUE, 2)?);
        }
    } else if ours {
        env.shift_remove(OUTPUT_ENV);
    }
    if env.is_empty() {
        settings.shift_remove("env");
    } else {
        settings.insert("env".to_string(), raw(&env, 1)?);
    }

    let mut out = serde_json::to_string_pretty(&settings)?;
    out.push('\n');
    Ok(out)
}

fn settings_path(scope: AgentScope) -> Result<PathBuf> {
    Ok(scope_root(scope)?.join(".claude").join("settings.json"))
}

/// Apply [`edit_settings`] to the settings file of `scope`. A file that does
/// not parse is left alone and reported; a file that would end up empty and
/// did not exist before is not created.
fn update_settings_file(scope: AgentScope, install: bool) -> Result<Option<PathBuf>> {
    let path = settings_path(scope)?;
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err).with_context(|| format!("Failed to read {}", path.display())),
    };
    if existing.is_none() && !install {
        return Ok(None);
    }
    let updated = edit_settings(existing.as_deref().unwrap_or_default(), install)
        .with_context(|| format!("{} was left unchanged", path.display()))?;
    if existing.as_deref() == Some(updated.as_str()) {
        return Ok(Some(path));
    }
    crate::fsutil::atomic_write(&path, updated.as_bytes(), true)?;
    Ok(Some(path))
}

// ============================================================================
// AGENTS.md block
// ============================================================================

/// `text` with the managed block present (`install`) or removed.
fn edit_instructions(text: &str, install: bool) -> String {
    let block = format!("{BLOCK_BEGIN}\n{INSTRUCTIONS}\n{BLOCK_END}");
    let existing = text.find("<!-- track:begin").and_then(|start| {
        text[start..]
            .find(BLOCK_END)
            .map(|end| (start, start + end + BLOCK_END.len()))
    });
    match (existing, install) {
        (Some((start, end)), true) => format!("{}{}{}", &text[..start], block, &text[end..]),
        (Some((start, end)), false) => {
            let before = text[..start].trim_end_matches('\n');
            let after = text[end..].trim_start_matches('\n');
            match (before.is_empty(), after.is_empty()) {
                (true, true) => String::new(),
                (true, false) => after.to_string(),
                (false, true) => format!("{before}\n"),
                (false, false) => format!("{before}\n\n{after}"),
            }
        }
        (None, true) if text.trim().is_empty() => format!("{block}\n"),
        (None, true) => format!("{}\n\n{block}\n", text.trim_end_matches('\n')),
        (None, false) => text.to_string(),
    }
}

fn update_instructions_file(install: bool) -> Result<Option<PathBuf>> {
    let path = scope_root(AgentScope::Project)?.join("AGENTS.md");
    let existing = std::fs::read_to_string(&path).ok();
    if existing.is_none() && !install {
        return Ok(None);
    }
    let updated = edit_instructions(existing.as_deref().unwrap_or_default(), install);
    if updated.is_empty() {
        // The block was all the file held, so the file was ours.
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
        return Ok(Some(path));
    }
    if existing.as_deref() != Some(updated.as_str()) {
        crate::fsutil::atomic_write(&path, updated.as_bytes(), true)?;
    }
    Ok(Some(path))
}

// ============================================================================
// Commands
// ============================================================================

#[derive(Serialize)]
struct SkillInstall {
    tool: &'static str,
    path: String,
}

#[derive(Serialize)]
struct InstallReport {
    success: bool,
    scope: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skills_installed: Vec<SkillInstall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_hook: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
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
            skills_only,
            hooks_only,
        } => install(*scope, !*hooks_only, !*skills_only, format),
        AgentCommands::Uninstall { scope } => uninstall(*scope, format),
        AgentCommands::Status => status(format),
    }
}

fn install(scope: AgentScope, skills: bool, hooks: bool, format: OutputFormat) -> Result<()> {
    let mut report = InstallReport {
        success: true,
        scope: scope_name(scope),
        skills_installed: Vec::new(),
        session_hook: None,
        instructions: None,
        notes: Vec::new(),
    };
    if skills {
        report.skills_installed = write_skills(scope)?
            .into_iter()
            .map(|(tool, path)| SkillInstall {
                tool,
                path: path.display().to_string(),
            })
            .collect();
    }
    if hooks {
        report.session_hook =
            update_settings_file(scope, true)?.map(|path| path.display().to_string());
        match scope {
            AgentScope::Project => {
                report.instructions =
                    update_instructions_file(true)?.map(|path| path.display().to_string());
                report.notes.push(
                    "Commit .claude/settings.json and AGENTS.md so the whole team gets them."
                        .to_string(),
                );
            }
            AgentScope::User => report.notes.push(
                "Tools without session hooks read AGENTS.md: run `track agent install --scope project` in a repository to add it there."
                    .to_string(),
            ),
        }
        report.notes.push(
            "The hook prints `track prime` in directories that have a .track.toml; run `track cache refresh` there once."
                .to_string(),
        );
    }

    match format {
        OutputFormat::Json => output_json(&report)?,
        OutputFormat::Text => {
            use colored::Colorize;
            for skill in &report.skills_installed {
                println!(
                    "{} {} skill: {}",
                    "Installed".green(),
                    skill.tool,
                    skill.path
                );
            }
            if let Some(path) = &report.session_hook {
                println!(
                    "{} Claude Code session hook ({HOOK_COMMAND}) and {OUTPUT_ENV}={OUTPUT_ENV_VALUE}: {path}",
                    "Installed".green()
                );
            }
            if let Some(path) = &report.instructions {
                println!("{} agent instructions: {path}", "Installed".green());
            }
            for note in &report.notes {
                println!("  {note}");
            }
        }
    }
    Ok(())
}

fn uninstall(scope: AgentScope, format: OutputFormat) -> Result<()> {
    let mut removed: Vec<String> = remove_skills(scope)?
        .into_iter()
        .map(|path| path.display().to_string())
        .collect();
    if let Some(path) = update_settings_file(scope, false)? {
        removed.push(format!("session hook in {}", path.display()));
    }
    if scope == AgentScope::Project
        && let Some(path) = update_instructions_file(false)?
    {
        removed.push(format!("instructions block in {}", path.display()));
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
struct ScopeStatus {
    scope: &'static str,
    skills: Vec<SkillStatus>,
    session_hook: bool,
    output_env: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions_block: Option<bool>,
}

#[derive(Serialize)]
struct SkillStatus {
    tool: &'static str,
    /// `current`, `outdated` (written by another version) or `missing`
    state: &'static str,
}

fn scope_status(scope: AgentScope) -> Result<ScopeStatus> {
    let root = scope_root(scope)?;
    let skills = skill_tools(scope)
        .iter()
        .map(|&(tool, tool_dir)| {
            let dir = root.join(tool_dir).join("skills").join("track");
            let state = if !dir.join("SKILL.md").is_file() {
                "missing"
            } else if SKILL_FILES.iter().all(|(relative, content)| {
                std::fs::read_to_string(dir.join(relative)).is_ok_and(|text| text == *content)
            }) {
                "current"
            } else {
                "outdated"
            };
            SkillStatus { tool, state }
        })
        .collect();
    let settings = std::fs::read_to_string(settings_path(scope)?).unwrap_or_default();
    let instructions_block = (scope == AgentScope::Project).then(|| {
        std::fs::read_to_string(root.join("AGENTS.md"))
            .is_ok_and(|text| text.contains("<!-- track:begin"))
    });
    Ok(ScopeStatus {
        scope: scope_name(scope),
        skills,
        session_hook: settings.contains(HOOK_COMMAND),
        output_env: settings.contains(&format!("\"{OUTPUT_ENV}\"")),
        instructions_block,
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
            let mark = |on: bool| if on { "yes" } else { "no" };
            for scope in &scopes {
                println!("{} scope:", scope.scope);
                for skill in &scope.skills {
                    println!("  {} skill: {}", skill.tool, skill.state);
                }
                println!("  Claude Code session hook: {}", mark(scope.session_hook));
                println!("  {OUTPUT_ENV} in settings: {}", mark(scope.output_env));
                if let Some(block) = scope.instructions_block {
                    println!("  AGENTS.md block: {}", mark(block));
                }
            }
            println!(
                "Session hooks are installed for Claude Code only; other tools get the skill and, in a project, AGENTS.md."
            );
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

    #[test]
    fn installing_into_an_empty_settings_file() {
        let out = edit_settings("", true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            json["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            HOOK_COMMAND
        );
        assert_eq!(
            json["hooks"]["SessionStart"][0]["hooks"][0]["type"],
            "command"
        );
        assert_eq!(json["env"][OUTPUT_ENV], OUTPUT_ENV_VALUE);
    }

    #[test]
    fn install_keeps_key_order_and_everything_it_does_not_own() {
        let before = r#"{
  "zeta": {"b": 1,   "a": 2},
  "hooks": {
    "PreToolUse": [{"matcher": "Bash", "hooks": []}],
    "SessionStart": [{"hooks": [{"type": "command", "command": "echo hi"}]}]
  },
  "alpha": true,
  "env": {"OTHER": "1"}
}"#;
        let after = edit_settings(before, true).unwrap();

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
        assert_eq!(json["env"][OUTPUT_ENV], OUTPUT_ENV_VALUE);

        // Installing again changes nothing.
        assert_eq!(edit_settings(&after, true).unwrap(), after);

        // Uninstalling removes exactly what was added.
        let removed = edit_settings(&after, false).unwrap();
        let json: serde_json::Value = serde_json::from_str(&removed).unwrap();
        assert_eq!(json["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert!(!removed.contains(HOOK_COMMAND));
        assert!(json["env"].get(OUTPUT_ENV).is_none());
        assert_eq!(json["env"]["OTHER"], "1");
    }

    #[test]
    fn uninstall_leaves_nothing_behind_in_a_file_it_created() {
        let installed = edit_settings("", true).unwrap();
        assert_eq!(edit_settings(&installed, false).unwrap(), "{}\n");
    }

    #[test]
    fn a_user_chosen_output_format_is_not_overwritten_or_removed() {
        let before = r#"{"env": {"TRACK_OUTPUT": "json"}}"#;
        let after = edit_settings(before, true).unwrap();
        let json: serde_json::Value = serde_json::from_str(&after).unwrap();
        assert_eq!(json["env"][OUTPUT_ENV], "json");
        let removed = edit_settings(&after, false).unwrap();
        assert!(removed.contains("\"json\""));
    }

    #[test]
    fn settings_that_do_not_parse_are_refused() {
        assert!(edit_settings("{ not json", true).is_err());
        assert!(edit_settings("[1, 2]", true).is_err());
        assert!(edit_settings(r#"{"hooks": []}"#, true).is_err());
    }

    #[test]
    fn the_instructions_block_is_added_replaced_and_removed_in_place() {
        let added = edit_instructions("# Project\n\nNotes.\n", true);
        assert!(added.starts_with("# Project\n\nNotes.\n\n<!-- track:begin"));
        assert!(added.ends_with("<!-- track:end -->\n"));
        assert_eq!(edit_instructions(&added, true), added);

        // An older block is replaced where it stands.
        let old = "# Project\n\n<!-- track:begin old -->\nstale\n<!-- track:end -->\n\n## After\n";
        let replaced = edit_instructions(old, true);
        assert!(!replaced.contains("stale"));
        assert!(replaced.contains("track prime"));
        assert!(replaced.ends_with("\n\n## After\n"));

        assert_eq!(edit_instructions(&added, false), "# Project\n\nNotes.\n");
        assert_eq!(edit_instructions(&edit_instructions("", true), false), "");
        assert_eq!(
            edit_instructions(&replaced, false),
            "# Project\n\n## After\n"
        );
    }
}
