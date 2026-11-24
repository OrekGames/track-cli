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

/// `track` run in `dir/repo` with its home in `dir/home`.
fn track(dir: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir.join("repo"))
        .env("HOME", dir.join("home"))
        .env("USERPROFILE", dir.join("home"))
        .env_remove("TRACK_MOCK_DIR")
        .env_remove("TRACK_OUTPUT");
    cmd
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn user_install_writes_skills_for_every_tool_and_the_claude_hook() {
    let dir = temp_dir();
    let home = dir.join("home");

    track(&dir).args(["agent", "install"]).assert().success();

    for tool in [".claude", ".copilot", ".cursor", ".gemini"] {
        let skill = home.join(tool).join("skills/track");
        assert!(skill.join("SKILL.md").is_file(), "{tool}");
        assert!(skill.join("references/jira.md").is_file(), "{tool}");
    }
    let settings = read_json(&home.join(".claude/settings.json"));
    let hook = &settings["hooks"]["SessionStart"][0]["hooks"][0];
    assert_eq!(hook["type"], "command");
    assert_eq!(hook["command"], "track prime --hook");
    assert_eq!(settings["env"]["TRACK_OUTPUT"], "agent");
    // The user scope never writes into the repository.
    assert!(fs::read_dir(dir.join("repo")).unwrap().next().is_none());

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
    track(&dir)
        .args(["agent", "install", "--hooks-only"])
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&settings_path).unwrap(), once);

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
    assert!(settings.get("env").is_none());
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
        .args(["agent", "install", "--hooks-only"])
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

    track(&dir)
        .args(["agent", "install", "--scope", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Commit .claude/settings.json and AGENTS.md",
        ));

    let agents = fs::read_to_string(repo.join("AGENTS.md")).unwrap();
    assert!(agents.starts_with("# Repo\n\nBuild with make.\n\n<!-- track:begin"));
    assert!(agents.contains("run `track prime`"));
    assert!(repo.join(".claude/skills/track/SKILL.md").is_file());
    assert_eq!(
        read_json(&repo.join(".claude/settings.json"))["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "track prime --hook"
    );
    // Nothing was written to the home directory.
    assert!(!dir.join("home/.claude").exists());

    let status = track(&dir)
        .args(["-o", "json", "agent", "status"])
        .output()
        .unwrap();
    let status: Value = serde_json::from_slice(&status.stdout).unwrap();
    let project = &status["scopes"][1];
    assert_eq!(project["scope"], "project");
    assert_eq!(project["session_hook"], true);
    assert_eq!(project["instructions_block"], true);
    assert_eq!(project["skills"][0]["state"], "current");
    assert_eq!(status["scopes"][0]["session_hook"], false);

    track(&dir)
        .args(["agent", "uninstall", "--scope", "project"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        "# Repo\n\nBuild with make.\n"
    );
    assert!(!repo.join(".claude/skills/track").exists());
    assert!(
        !fs::read_to_string(repo.join(".claude/settings.json"))
            .unwrap()
            .contains("track prime")
    );

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
        .stdout(predicate::str::starts_with("# GitHub\n"))
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
