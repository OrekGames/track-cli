//! `track agent install | uninstall | status` and `track guide`.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "track-agent-install-test-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    fs::create_dir_all(dir.join("home")).unwrap();
    fs::create_dir_all(dir.join("repo")).unwrap();
    dir
}

/// `track` run in `dir/repo` with its home in `dir/home`. Every variable the
/// installer reads a location from is pinned, so the test sees the same
/// paths on every platform and on a developer's machine, and none that a
/// Claude Code session running the tests sets (`CLAUDE_ENV_FILE`) leaks in.
fn track(dir: &Path) -> assert_cmd::Command {
    let home = dir.join("home");
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir.join("repo"))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("OREK_HOME", home.join("orek"));
    for var in [
        "CODEX_HOME",
        "XDG_CONFIG_HOME",
        "APPDATA",
        "HOMEDRIVE",
        "HOMEPATH",
        "TRACK_MOCK_DIR",
        "TRACK_CACHE_DIR",
        "TRACK_OUTPUT",
        "TRACK_ERROR_FORMAT",
        "TRACKER_CONFIG",
        "TRACKER_BACKEND",
        "CLAUDE_ENV_FILE",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn status(dir: &Path) -> Value {
    let out = track(dir)
        .args(["-o", "json", "agent", "status"])
        .output()
        .unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}

fn tool_status<'a>(status: &'a Value, scope: usize, tool: &str) -> &'a Value {
    status["scopes"][scope]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tool"] == tool)
        .unwrap_or_else(|| panic!("no status for {tool}"))
}

#[test]
fn user_install_sets_up_every_tool_where_its_docs_say() {
    let dir = temp_dir();
    let home = dir.join("home");

    track(&dir).args(["agent", "install"]).assert().success();

    // One skill copy per folder the tools read.
    for skills in [
        "orek/config/skills",
        ".claude/skills",
        ".agents/skills",
        ".gemini/config/skills",
        ".copilot/skills",
    ] {
        let skill = home.join(skills).join("track");
        assert!(skill.join("SKILL.md").is_file(), "{skills}");
        assert!(skill.join("references/jira.md").is_file(), "{skills}");
    }
    // Cursor and Gemini CLI read ~/.agents/skills; no second copy.
    assert!(!home.join(".cursor/skills").exists());
    assert!(!home.join(".gemini/skills").exists());

    // Session-start hooks, each in its tool's own shape.
    let claude = read_json(&home.join(".claude/settings.json"));
    let hook = &claude["hooks"]["SessionStart"][0]["hooks"][0];
    assert_eq!(hook["type"], "command");
    assert_eq!(hook["command"], "track prime --hook=claude");
    assert!(claude.get("env").is_none());
    let codex = read_json(&home.join(".codex/hooks.json"));
    assert_eq!(
        codex["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "track prime --hook=codex"
    );
    let gemini = read_json(&home.join(".gemini/settings.json"));
    assert_eq!(
        gemini["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "track prime --hook=gemini"
    );
    let cursor = read_json(&home.join(".cursor/hooks.json"));
    assert_eq!(cursor["version"], 1);
    assert_eq!(
        cursor["hooks"]["sessionStart"][0]["command"],
        "track prime --hook=cursor"
    );
    let antigravity = read_json(&home.join(".gemini/config/hooks.json"));
    assert_eq!(
        antigravity["track"]["SessionStart"][0]["command"],
        "track prime --hook=antigravity"
    );

    // Codex gets the output format from its shell environment policy.
    let config = fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    assert!(config.contains("TRACK_OUTPUT = \"agent\""), "{config}");

    // User instructions, conditional on a .track.toml.
    for file in [".claude/CLAUDE.md", ".codex/AGENTS.md", ".gemini/GEMINI.md"] {
        let text = fs::read_to_string(home.join(file)).unwrap();
        assert!(text.contains("<!-- track:begin"), "{file}");
        assert!(text.contains("`.track.toml`"), "{file}");
    }

    // The user scope never writes into the repository.
    assert!(fs::read_dir(dir.join("repo")).unwrap().next().is_none());

    let status = status(&dir);
    let orek = tool_status(&status, 0, "Orek.ai");
    assert_eq!(orek["skill"], "current");
    assert_eq!(orek["session_hook"], "not available");
    let codex = tool_status(&status, 0, "Codex");
    assert_eq!(codex["session_hook"], "installed");
    assert_eq!(codex["instructions"], "installed");
    assert_eq!(codex["environment"], "set");

    // Uninstall takes back everything, and only that.
    track(&dir).args(["agent", "uninstall"]).assert().success();
    for file in [
        ".claude/settings.json",
        ".codex/hooks.json",
        ".codex/config.toml",
        ".codex/AGENTS.md",
        ".cursor/hooks.json",
        ".gemini/settings.json",
        ".gemini/config/hooks.json",
        ".claude/CLAUDE.md",
        ".gemini/GEMINI.md",
    ] {
        assert!(!home.join(file).exists(), "{file} left behind");
    }
    for skills in ["orek/config/skills", ".claude/skills", ".agents/skills"] {
        assert!(!home.join(skills).join("track").exists(), "{skills}");
    }

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn uninstalling_one_tool_keeps_what_the_others_still_read() {
    let dir = temp_dir();
    let home = dir.join("home");
    track(&dir).args(["agent", "install"]).assert().success();

    // Codex shares ~/.agents/skills with Cursor and Gemini CLI.
    let out = track(&dir)
        .args(["-o", "json", "agent", "uninstall", "--tools", "codex"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(home.join(".agents/skills/track/SKILL.md").is_file());
    let kept = report["kept"].as_array().unwrap();
    assert!(
        kept.iter().any(|item| {
            let item = item.as_str().unwrap();
            item.contains(".agents/skills/track") && item.contains("Cursor and Gemini CLI")
        }),
        "{report}"
    );
    // What only Codex reads is gone.
    assert!(!home.join(".codex/hooks.json").exists());
    assert!(!home.join(".codex/AGENTS.md").exists());
    assert!(!home.join(".codex/config.toml").exists());
    let status = status(&dir);
    assert_eq!(tool_status(&status, 0, "Cursor")["skill"], "current");
    assert_eq!(tool_status(&status, 0, "Gemini CLI")["skill"], "current");

    // GEMINI.md serves Antigravity and Gemini CLI: it goes once both are named.
    track(&dir)
        .args(["agent", "uninstall", "--tools", "gemini"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Kept instructions block in"))
        .stdout(predicate::str::contains("(Antigravity also reads it)"))
        .stdout(predicate::str::contains("name those tools in --tools too"));
    assert!(home.join(".gemini/GEMINI.md").is_file());
    assert!(!home.join(".gemini/settings.json").exists());
    track(&dir)
        .args(["agent", "uninstall", "--tools", "gemini,antigravity"])
        .assert()
        .success();
    assert!(!home.join(".gemini/GEMINI.md").exists());
    assert!(!home.join(".gemini/config/hooks.json").exists());

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn uninstalling_one_tool_from_a_project_keeps_the_shared_agents_md_block() {
    let dir = temp_dir();
    let repo = dir.join("repo");
    track(&dir)
        .args(["agent", "install", "--scope", "project"])
        .assert()
        .success();

    // Every other tool reads AGENTS.md and .agents/skills too.
    track(&dir)
        .args([
            "agent",
            "uninstall",
            "--scope",
            "project",
            "--tools",
            "cursor",
        ])
        .assert()
        .success();
    assert!(
        fs::read_to_string(repo.join("AGENTS.md"))
            .unwrap()
            .contains("<!-- track:begin")
    );
    assert!(repo.join(".agents/skills/track/SKILL.md").is_file());
    assert!(!repo.join(".cursor").exists());
    let status = status(&dir);
    assert_eq!(
        tool_status(&status, 1, "Codex")["instructions"],
        "installed"
    );
    assert_eq!(tool_status(&status, 1, "Codex")["skill"], "current");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn uninstall_prunes_only_folders_on_the_named_tools_paths() {
    let dir = temp_dir();
    let repo = dir.join("repo");
    // An empty folder of the user's, for a tool not being set up.
    fs::create_dir(repo.join(".gemini")).unwrap();

    track(&dir)
        .args([
            "agent", "install", "--scope", "project", "--tools", "claude",
        ])
        .assert()
        .success();
    track(&dir)
        .args([
            "agent",
            "uninstall",
            "--scope",
            "project",
            "--tools",
            "claude",
        ])
        .assert()
        .success();

    assert!(!repo.join(".claude").exists());
    assert!(repo.join(".gemini").is_dir());

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_settings_file_with_comments_skips_that_file_and_sets_up_the_rest() {
    // Gemini CLI's settings.json allows comments, which a rewrite would lose.
    let dir = temp_dir();
    let home = dir.join("home");
    let settings = home.join(".gemini/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let commented = "{\n  // my theme\n  \"theme\": \"dark\"\n}\n";
    fs::write(&settings, commented).unwrap();

    let out = track(&dir)
        .args(["-o", "json", "agent", "install"])
        .output()
        .unwrap();

    // Partial: exit 6, the report on stdout, one JSON error on stderr.
    assert_eq!(out.status.code(), Some(6));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["success"], false);
    let gemini = report["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["tool"] == "Gemini CLI")
        .unwrap();
    let error = gemini["errors"][0].as_str().unwrap();
    assert!(
        error.contains("settings.json was left unchanged"),
        "{error}"
    );
    assert!(error.contains("comments"), "{error}");
    let stderr: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "partial_failure");

    // That file is untouched; everything else, Gemini's GEMINI.md included,
    // is in place.
    assert_eq!(fs::read_to_string(&settings).unwrap(), commented);
    assert!(
        fs::read_to_string(home.join(".gemini/GEMINI.md"))
            .unwrap()
            .contains("<!-- track:begin")
    );
    assert!(home.join(".agents/skills/track/SKILL.md").is_file());
    assert!(home.join(".claude/settings.json").is_file());
    assert!(home.join(".cursor/hooks.json").is_file());
    assert!(home.join(".gemini/config/hooks.json").is_file());

    // Text mode says the same, and still exits non-zero.
    track(&dir)
        .args(["agent", "install", "--tools", "gemini"])
        .assert()
        .code(6)
        .stdout(predicate::str::contains("Not set up:"))
        .stderr(predicate::str::contains("Code: partial_failure"));

    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn install_keeps_the_permissions_of_the_users_files_and_folders() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let dir = temp_dir();
    let repo = dir.join("repo");
    let home = dir.join("home");
    fs::set_permissions(&repo, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(repo.join("AGENTS.md"), "# Repo\n").unwrap();
    fs::set_permissions(repo.join("AGENTS.md"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::create_dir_all(home.join(".claude")).unwrap();
    fs::set_permissions(home.join(".claude"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(home.join(".claude/settings.json"), "{}\n").unwrap();
    fs::set_permissions(
        home.join(".claude/settings.json"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();

    track(&dir)
        .args(["agent", "install", "--scope", "project", "--tools", "codex"])
        .assert()
        .success();
    track(&dir)
        .args(["agent", "install", "--tools", "claude"])
        .assert()
        .success();

    assert_eq!(mode(&repo), 0o755);
    assert_eq!(mode(&repo.join("AGENTS.md")), 0o644);
    assert!(
        fs::read_to_string(repo.join("AGENTS.md"))
            .unwrap()
            .contains("<!-- track:begin")
    );
    assert_eq!(mode(&home.join(".claude")), 0o755);
    assert_eq!(mode(&home.join(".claude/settings.json")), 0o640);
    // What the installer creates gets the default mode, as anything else
    // this umask makes would, rather than a private one.
    let reference = dir.join("reference");
    fs::write(&reference, "").unwrap();
    fs::create_dir(dir.join("reference-dir")).unwrap();
    assert_eq!(mode(&repo.join(".codex/hooks.json")), mode(&reference));
    assert_eq!(mode(&repo.join(".codex")), mode(&dir.join("reference-dir")));

    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_symlinked_settings_file_stays_a_link_to_the_edited_dotfile() {
    let dir = temp_dir();
    let home = dir.join("home");
    let dotfile = home.join("dotfiles/claude-settings.json");
    fs::create_dir_all(dotfile.parent().unwrap()).unwrap();
    fs::write(&dotfile, "{\"model\": \"opus\"}\n").unwrap();
    fs::create_dir_all(home.join(".claude")).unwrap();
    let link = home.join(".claude/settings.json");
    std::os::unix::fs::symlink("../dotfiles/claude-settings.json", &link).unwrap();

    track(&dir)
        .args(["agent", "install", "--hooks-only", "--tools", "claude"])
        .assert()
        .success();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let settings = read_json(&dotfile);
    assert_eq!(settings["model"], "opus");
    assert_eq!(
        settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "track prime --hook=claude"
    );

    track(&dir)
        .args(["agent", "uninstall", "--tools", "claude"])
        .assert()
        .success();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(read_json(&dotfile), serde_json::json!({ "model": "opus" }));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn tools_limits_install_to_the_named_tools() {
    let dir = temp_dir();
    let home = dir.join("home");

    track(&dir)
        .args(["agent", "install", "--tools", "orek,cursor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Orek.ai"))
        .stdout(predicate::str::contains("no session-start hook"));

    assert!(home.join("orek/config/skills/track/SKILL.md").is_file());
    assert!(home.join(".agents/skills/track/SKILL.md").is_file());
    assert!(home.join(".cursor/hooks.json").is_file());
    assert!(!home.join(".claude").exists());
    assert!(!home.join(".codex").exists());
    assert!(!home.join(".gemini").exists());

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_skill_left_by_an_earlier_version_is_moved_not_duplicated() {
    let dir = temp_dir();
    let home = dir.join("home");
    for old in [".cursor/skills/track", ".gemini/skills/track"] {
        fs::create_dir_all(home.join(old)).unwrap();
        fs::write(
            home.join(old).join("SKILL.md"),
            "---\nname: track\ndescription: old\n---\n",
        )
        .unwrap();
    }
    // A different skill that happens to share the folder name stays.
    fs::create_dir_all(home.join(".cursor/skills/other")).unwrap();

    track(&dir)
        .args(["agent", "install", "--skills-only"])
        .assert()
        .success();
    assert!(!home.join(".cursor/skills/track").exists());
    assert!(!home.join(".gemini/skills/track").exists());
    assert!(home.join(".cursor/skills/other").exists());
    assert!(home.join(".agents/skills/track/SKILL.md").is_file());

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn install_is_idempotent_and_preserves_existing_settings() {
    let dir = temp_dir();
    let settings_path = dir.join("home/.claude/settings.json");
    fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
    fs::write(
        &settings_path,
        r#"{
  "permissions": {"allow": ["Bash(git status)"]},
  "hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "echo mine"}]}]},
  "model": "opus"
}
"#,
    )
    .unwrap();

    track(&dir)
        .args(["agent", "install", "--hooks-only"])
        .assert()
        .success();
    let once = fs::read_to_string(&settings_path).unwrap();
    let codex_once = fs::read_to_string(dir.join("home/.codex/config.toml")).unwrap();
    track(&dir)
        .args(["agent", "install", "--hooks-only"])
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&settings_path).unwrap(), once);
    assert_eq!(
        fs::read_to_string(dir.join("home/.codex/config.toml")).unwrap(),
        codex_once
    );

    let settings = read_json(&settings_path);
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["permissions"]["allow"][0], "Bash(git status)");
    let hooks = settings["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(hooks.len(), 2);
    assert_eq!(hooks[0]["hooks"][0]["command"], "echo mine");
    assert!(once.find("\"permissions\"").unwrap() < once.find("\"hooks\"").unwrap());
    assert!(once.find("\"hooks\"").unwrap() < once.find("\"model\"").unwrap());
    // --hooks-only installs no skill.
    assert!(!dir.join("home/.claude/skills").exists());

    track(&dir).args(["agent", "uninstall"]).assert().success();
    let settings = read_json(&settings_path);
    assert_eq!(
        settings["hooks"]["SessionStart"].as_array().unwrap().len(),
        1
    );
    assert_eq!(settings["model"], "opus");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_settings_file_that_does_not_parse_is_left_untouched() {
    let dir = temp_dir();
    let settings_path = dir.join("home/.claude/settings.json");
    fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
    fs::write(&settings_path, "{ broken").unwrap();

    track(&dir)
        .args(["agent", "install", "--hooks-only", "--tools", "claude"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("was left unchanged"));

    assert_eq!(fs::read_to_string(&settings_path).unwrap(), "{ broken");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn project_install_adds_shared_files_and_uninstall_restores_the_repository() {
    let dir = temp_dir();
    let repo = dir.join("repo");
    fs::write(repo.join("AGENTS.md"), "# Repo\n\nBuild with make.\n").unwrap();
    // Claude Code reads CLAUDE.md instead of AGENTS.md when one exists.
    fs::write(repo.join("CLAUDE.md"), "# Claude notes\n").unwrap();

    track(&dir)
        .args(["agent", "install", "--scope", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Commit the files listed above"));

    let agents = fs::read_to_string(repo.join("AGENTS.md")).unwrap();
    assert!(agents.starts_with("# Repo\n\nBuild with make.\n\n<!-- track:begin"));
    assert!(agents.contains("run `track prime`"));
    let claude_md = fs::read_to_string(repo.join("CLAUDE.md")).unwrap();
    assert!(claude_md.contains("\n@AGENTS.md\n"), "{claude_md}");
    // No GEMINI.md existed, so none is created.
    assert!(!repo.join("GEMINI.md").exists());

    assert!(repo.join(".orek/skills/track/SKILL.md").is_file());
    assert!(repo.join(".claude/skills/track/SKILL.md").is_file());
    assert!(repo.join(".agents/skills/track/SKILL.md").is_file());
    for (file, pointer) in [
        (
            ".claude/settings.json",
            "/hooks/SessionStart/0/hooks/0/command",
        ),
        (".codex/hooks.json", "/hooks/SessionStart/0/hooks/0/command"),
        (
            ".gemini/settings.json",
            "/hooks/SessionStart/0/hooks/0/command",
        ),
        (".cursor/hooks.json", "/hooks/sessionStart/0/command"),
        (".agents/hooks.json", "/track/SessionStart/0/command"),
    ] {
        let command = read_json(&repo.join(file))
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string);
        assert!(
            command.is_some_and(|c| c.starts_with("track prime --hook=")),
            "{file}"
        );
    }
    // Nothing was written to the home directory.
    assert!(fs::read_dir(dir.join("home")).unwrap().next().is_none());

    let status = status(&dir);
    assert_eq!(status["scopes"][1]["scope"], "project");
    let claude = tool_status(&status, 1, "Claude Code");
    assert_eq!(claude["skill"], "current");
    assert_eq!(claude["session_hook"], "installed");
    assert_eq!(claude["instructions"], "installed");
    assert_eq!(
        tool_status(&status, 0, "Claude Code")["session_hook"],
        "missing"
    );

    track(&dir)
        .args(["agent", "uninstall", "--scope", "project"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        "# Repo\n\nBuild with make.\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("CLAUDE.md")).unwrap(),
        "# Claude notes\n"
    );
    // Every file and folder the install created is gone again.
    let mut left: Vec<String> = fs::read_dir(&repo)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["AGENTS.md", "CLAUDE.md"]);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_agents_file_that_only_held_the_block_is_removed_with_it() {
    let dir = temp_dir();
    let repo = dir.join("repo");

    track(&dir)
        .args(["agent", "install", "--scope", "project", "--hooks-only"])
        .assert()
        .success();
    assert!(repo.join("AGENTS.md").is_file());

    track(&dir)
        .args(["agent", "uninstall", "--scope", "project"])
        .assert()
        .success();
    assert!(!repo.join("AGENTS.md").exists());

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn init_skills_still_installs_the_whole_skill() {
    let dir = temp_dir();

    track(&dir).args(["init", "--skills"]).assert().success();

    let skill = dir.join("home/.claude/skills/track");
    assert!(skill.join("SKILL.md").is_file());
    assert!(skill.join("references/output.md").is_file());
    // It installs skills only, as before.
    assert!(!dir.join("home/.claude/settings.json").exists());

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn guide_lists_topics_and_prints_one() {
    let dir = temp_dir();

    track(&dir)
        .arg("guide")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "  jira: Read when the project uses Jira",
        ))
        .stdout(predicate::str::contains("  output: "));

    track(&dir)
        .args(["guide", "GitHub"])
        .assert()
        .success()
        // The text is embedded from files that a Windows checkout gives CRLF
        // line endings, so only the title is asserted, not its line ending.
        .stdout(predicate::str::starts_with("# GitHub"))
        .stdout(predicate::str::contains("is:open"));

    track(&dir)
        .args(["guide", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "No guide topic 'nope'. Topics: youtrack, jira",
        ))
        .stderr(predicate::str::contains("Code: unknown_topic"));

    fs::remove_dir_all(&dir).unwrap();
}
