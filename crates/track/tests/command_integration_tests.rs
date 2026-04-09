//! Integration tests for the config, init, open, and issue-shortcut commands.
//!
//! These test the command handlers extracted during the main.rs refactoring.
//! All tests use temp directories for isolation and don't require a real backend.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

// =============================================================================
// Helpers
// =============================================================================

/// Create a unique temp directory for test isolation.
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
        "track-cmd-test-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Build a track command running in the given directory.
/// Clears env vars so only local config matters.
fn track_in(dir: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env_remove("TRACKER_URL")
        .env_remove("TRACKER_TOKEN")
        .env_remove("TRACKER_BACKEND")
        .env_remove("TRACKER_CONFIG")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .env_remove("JIRA_URL")
        .env_remove("JIRA_EMAIL")
        .env_remove("JIRA_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GITHUB_OWNER")
        .env_remove("GITHUB_REPO")
        .env_remove("GITHUB_API_URL")
        .env_remove("GITLAB_TOKEN")
        .env_remove("GITLAB_URL")
        .env_remove("GITLAB_PROJECT_ID")
        .env_remove("GITLAB_NAMESPACE")
        .env_remove("TRACK_MOCK_DIR");
    cmd
}

/// Write a minimal .track.toml in the given directory.
fn write_config(dir: &Path, content: &str) {
    fs::write(dir.join(".track.toml"), content).unwrap();
}

/// Get the path to the fixtures directory.
fn fixtures_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("scenarios")
}

// =============================================================================
// config keys
// =============================================================================

#[test]
fn test_config_keys_text_output() {
    let dir = temp_dir();
    track_in(&dir)
        .args(["config", "keys"])
        .assert()
        .success()
        .stdout(predicate::str::contains("backend"))
        .stdout(predicate::str::contains("url"))
        .stdout(predicate::str::contains("token"))
        .stdout(predicate::str::contains("default_project"))
        .stdout(predicate::str::contains("youtrack.url"))
        .stdout(predicate::str::contains("jira.url"))
        .stdout(predicate::str::contains("github.token"))
        .stdout(predicate::str::contains("gitlab.token"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_keys_json_output() {
    let dir = temp_dir();
    let output = track_in(&dir)
        .args(["-o", "json", "config", "keys"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let keys = json.as_array().unwrap();
    assert!(keys.len() >= 10, "Should list many config keys");

    // Verify structure
    let first = &keys[0];
    assert!(first.get("key").is_some());
    assert!(first.get("type").is_some());
    assert!(first.get("description").is_some());

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config set / get round-trip
// =============================================================================

#[test]
fn test_config_set_and_get_round_trip() {
    let dir = temp_dir();
    // Create a minimal .track.toml so set can update it
    write_config(&dir, "");

    // Set a value
    track_in(&dir)
        .args(["config", "set", "default_project", "MYPROJ"])
        .assert()
        .success()
        .stdout(predicate::str::contains("MYPROJ"));

    // Get the value back
    track_in(&dir)
        .args(["config", "get", "default_project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("MYPROJ"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_set_and_get_json_output() {
    let dir = temp_dir();
    write_config(&dir, "");

    // Set
    track_in(&dir)
        .args(["-o", "json", "config", "set", "url", "https://example.com"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""success": true"#));

    // Get
    let output = track_in(&dir)
        .args(["-o", "json", "config", "get", "url"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["key"], "url");
    assert_eq!(json["value"], "https://example.com");
    assert_eq!(json["is_set"], true);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_get_unset_key() {
    let dir = temp_dir();
    write_config(&dir, "");

    track_in(&dir)
        .args(["config", "get", "default_project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not set"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_get_invalid_key() {
    let dir = temp_dir();
    track_in(&dir)
        .args(["config", "get", "nonexistent_key"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid configuration key"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_set_invalid_key() {
    let dir = temp_dir();
    track_in(&dir)
        .args(["config", "set", "bogus_key", "value"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid configuration key"));

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config set backend
// =============================================================================

#[test]
fn test_config_set_backend_value() {
    let dir = temp_dir();
    write_config(&dir, "");

    track_in(&dir)
        .args(["config", "set", "backend", "jira"])
        .assert()
        .success();

    // Verify via get
    track_in(&dir)
        .args(["config", "get", "backend"])
        .assert()
        .success()
        .stdout(predicate::str::contains("jira"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_set_backend_invalid_value() {
    let dir = temp_dir();
    write_config(&dir, "");

    track_in(&dir)
        .args(["config", "set", "backend", "nosuchbackend"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid backend"));

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config secret masking
// =============================================================================

#[test]
fn test_config_get_masks_token_in_text() {
    let dir = temp_dir();
    write_config(&dir, "token = \"super-secret-token\"\n");

    // Text output should mask the token
    track_in(&dir)
        .args(["config", "get", "token"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hidden"))
        .stdout(predicate::str::contains("super-secret").not());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_get_shows_token_in_json() {
    let dir = temp_dir();
    write_config(&dir, "token = \"super-secret-token\"\n");

    // JSON output should include the actual value
    let output = track_in(&dir)
        .args(["-o", "json", "config", "get", "token"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["value"], "super-secret-token");

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config show
// =============================================================================

#[test]
fn test_config_show_with_config() {
    let dir = temp_dir();
    write_config(
        &dir,
        "backend = \"youtrack\"\nurl = \"https://yt.example.com\"\ndefault_project = \"DEMO\"\n",
    );

    track_in(&dir)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("youtrack"))
        .stdout(predicate::str::contains("DEMO"))
        .stdout(predicate::str::contains("yt.example.com"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_show_no_config() {
    let dir = temp_dir();

    track_in(&dir)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No configuration found"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_show_json_output() {
    let dir = temp_dir();
    write_config(
        &dir,
        "backend = \"jira\"\nurl = \"https://jira.example.com\"\ndefault_project = \"SMS\"\n",
    );

    let output = track_in(&dir)
        .args(["-o", "json", "config", "show"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // New format: { "config": [{ "key": "...", "value": "...", "source": "..." }, ...] }
    let config = json["config"]
        .as_array()
        .expect("config should be an array");
    let find_val = |key: &str| -> Option<String> {
        config
            .iter()
            .find(|e| e["key"] == key)
            .and_then(|e| e["value"].as_str().map(|s| s.to_string()))
    };
    assert_eq!(find_val("backend"), Some("jira".to_string()));
    assert_eq!(find_val("default_project"), Some("SMS".to_string()));
    assert_eq!(
        find_val("url"),
        Some("https://jira.example.com".to_string())
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config clear
// =============================================================================

#[test]
fn test_config_clear_removes_defaults() {
    let dir = temp_dir();
    write_config(
        &dir,
        "backend = \"youtrack\"\nurl = \"https://yt.example.com\"\ntoken = \"tok\"\ndefault_project = \"DEMO\"\n",
    );

    track_in(&dir)
        .args(["config", "clear"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cleared"));

    // Verify default_project and backend are gone but url/token remain
    let content = fs::read_to_string(dir.join(".track.toml")).unwrap();
    assert!(
        !content.contains("default_project"),
        "default_project should be cleared"
    );
    assert!(
        content.contains("url"),
        "url should be preserved after clear"
    );
    assert!(
        content.contains("token"),
        "token should be preserved after clear"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_clear_no_config() {
    let dir = temp_dir();

    track_in(&dir)
        .args(["config", "clear"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No .track.toml"));

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config path
// =============================================================================

#[test]
fn test_config_path_output() {
    let dir = temp_dir();

    let output = track_in(&dir).args(["config", "path"]).output().unwrap();
    assert!(output.status.success());
    let path_str = String::from_utf8(output.stdout).unwrap();
    // Output now shows both global and project paths
    assert!(
        path_str.contains("Global:") && path_str.contains("Project:"),
        "config path should show Global and Project lines, got: {}",
        path_str.trim()
    );
    assert!(
        path_str.contains(".track.toml"),
        "config path should reference .track.toml, got: {}",
        path_str.trim()
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config backend subcommand
// =============================================================================

#[test]
fn test_config_backend_subcommand() {
    let dir = temp_dir();
    write_config(&dir, "backend = \"youtrack\"\n");

    track_in(&dir)
        .args(["config", "backend", "github"])
        .assert()
        .success()
        .stdout(predicate::str::contains("github"));

    // Verify it was persisted
    let content = fs::read_to_string(dir.join(".track.toml")).unwrap();
    assert!(
        content.contains("github"),
        "Backend should be updated to github"
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config does not need API
// =============================================================================

#[test]
fn test_config_commands_work_without_url_or_token() {
    let dir = temp_dir();

    // These should all succeed even without any URL/token configured
    track_in(&dir).args(["config", "keys"]).assert().success();
    track_in(&dir).args(["config", "show"]).assert().success();
    track_in(&dir).args(["config", "path"]).assert().success();

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// init command
// =============================================================================

#[test]
fn test_init_creates_config_and_guide() {
    let dir = temp_dir();

    track_in(&dir)
        .args([
            "init",
            "--url",
            "https://youtrack.example.com",
            "--token",
            "perm:test-token",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(".track.toml"))
        .stdout(predicate::str::contains("AGENT_GUIDE.md"));

    // Verify files were created
    assert!(dir.join(".track.toml").exists(), ".track.toml should exist");
    assert!(
        dir.join("AGENT_GUIDE.md").exists(),
        "AGENT_GUIDE.md should exist"
    );

    // Verify config content
    let content = fs::read_to_string(dir.join(".track.toml")).unwrap();
    assert!(content.contains("youtrack.example.com"));
    assert!(content.contains("perm:test-token"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_json_output() {
    let dir = temp_dir();

    let output = track_in(&dir)
        .args([
            "-o",
            "json",
            "init",
            "--url",
            "https://yt.test",
            "--token",
            "tok",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["backend"], "youtrack");
    assert!(
        json["config_path"]
            .as_str()
            .unwrap()
            .contains(".track.toml")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_fails_if_config_exists() {
    let dir = temp_dir();
    write_config(&dir, "url = \"https://existing.com\"\n");

    track_in(&dir)
        .args(["init", "--url", "https://new.com", "--token", "tok"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_rejects_invalid_url() {
    let dir = temp_dir();

    track_in(&dir)
        .args(["init", "--url", "not-a-url", "--token", "tok"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid URL"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_with_backend_flag() {
    let dir = temp_dir();

    track_in(&dir)
        .args([
            "init",
            "--url",
            "https://company.atlassian.net",
            "--token",
            "jira-tok",
            "-b",
            "jira",
            "-e",
            "user@company.com",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(dir.join(".track.toml")).unwrap();
    assert!(content.contains("jira"), "Config should have jira backend");
    assert!(
        content.contains("company.atlassian.net"),
        "Config should have the URL"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_jira_requires_email() {
    let dir = temp_dir();

    track_in(&dir)
        .args([
            "init",
            "--url",
            "https://company.atlassian.net",
            "--token",
            "tok",
            "-b",
            "jira",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("email"));

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// init --skills
// =============================================================================

#[test]
fn test_init_skills_only_mode() {
    let dir = temp_dir();

    // --skills without --url/--token should install skills and succeed
    track_in(&dir)
        .args(["init", "--skills"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skill").or(predicate::str::contains("Installed")));

    // Should NOT create .track.toml
    assert!(
        !dir.join(".track.toml").exists(),
        ".track.toml should not be created in skills-only mode"
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// open command help
// =============================================================================

#[test]
fn test_open_command_help() {
    cargo_bin_cmd!("track")
        .args(["open", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Open"))
        .stdout(predicate::str::contains("browser"));
}

// =============================================================================
// Command help tests for new commands
// =============================================================================

#[test]
fn test_config_command_help() {
    cargo_bin_cmd!("track")
        .args(["config", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("set"))
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("keys"))
        .stdout(predicate::str::contains("clear"))
        .stdout(predicate::str::contains("path"))
        .stdout(predicate::str::contains("test"))
        .stdout(predicate::str::contains("backend"))
        .stdout(predicate::str::contains("project"));
}

#[test]
fn test_config_alias_cfg() {
    cargo_bin_cmd!("track")
        .args(["cfg", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("set"))
        .stdout(predicate::str::contains("show"));
}

#[test]
fn test_init_command_help() {
    cargo_bin_cmd!("track")
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--url"))
        .stdout(predicate::str::contains("--token"))
        .stdout(predicate::str::contains("--project"))
        .stdout(predicate::str::contains("--backend"))
        .stdout(predicate::str::contains("--skills"));
}

#[test]
fn test_cache_command_help() {
    cargo_bin_cmd!("track")
        .args(["cache", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("refresh"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("path"));
}

#[test]
fn test_eval_command_help() {
    cargo_bin_cmd!("track")
        .args(["eval", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("clear"))
        .stdout(predicate::str::contains("status"));
}

// =============================================================================
// Issue shortcut (External subcommand)
// =============================================================================

#[test]
fn test_issue_shortcut_with_mock() {
    let dir = temp_dir();
    let scenario = fixtures_path().join("basic-workflow");

    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .arg("DEMO-1")
        .assert()
        .success()
        .stdout(predicate::str::contains("DEMO-1"))
        .stdout(predicate::str::contains("Implement user authentication"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_shortcut_with_full_flag() {
    let dir = temp_dir();
    let scenario = fixtures_path().join("basic-workflow");

    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args(["DEMO-1", "--full"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DEMO-1"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_shortcut_json_output() {
    let dir = temp_dir();
    let scenario = fixtures_path().join("basic-workflow");

    let output = cargo_bin_cmd!("track")
        .current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args([
            "-o",
            "json",
            "--url",
            "https://mock.test",
            "--token",
            "mock-token",
        ])
        .arg("DEMO-1")
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["id_readable"], "DEMO-1");

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// Invalid subcommand rejection
// =============================================================================

#[test]
fn test_non_issue_id_rejected() {
    cargo_bin_cmd!("track")
        .arg("foobar")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn test_bare_number_rejected() {
    cargo_bin_cmd!("track")
        .arg("123")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

// =============================================================================
// config set with backend-specific keys
// =============================================================================

#[test]
fn test_config_set_nested_backend_keys() {
    let dir = temp_dir();
    write_config(&dir, "");

    // Set github.owner
    track_in(&dir)
        .args(["config", "set", "github.owner", "myorg"])
        .assert()
        .success();

    // Set github.repo
    track_in(&dir)
        .args(["config", "set", "github.repo", "myrepo"])
        .assert()
        .success();

    // Verify via get
    track_in(&dir)
        .args(["config", "get", "github.owner"])
        .assert()
        .success()
        .stdout(predicate::str::contains("myorg"));

    track_in(&dir)
        .args(["config", "get", "github.repo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("myrepo"));

    // Verify the TOML file has the [github] section
    let content = fs::read_to_string(dir.join(".track.toml")).unwrap();
    assert!(content.contains("[github]"), "Should have [github] section");
    assert!(content.contains("myorg"));
    assert!(content.contains("myrepo"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_set_gitlab_keys() {
    let dir = temp_dir();
    write_config(&dir, "");

    track_in(&dir)
        .args(["config", "set", "gitlab.url", "https://gitlab.example.com"])
        .assert()
        .success();

    track_in(&dir)
        .args(["config", "set", "gitlab.project_id", "42"])
        .assert()
        .success();

    let output = track_in(&dir)
        .args(["-o", "json", "config", "get", "gitlab.project_id"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["value"], "42");

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config show with backend-specific sections
// =============================================================================

#[test]
fn test_config_show_with_github_section() {
    let dir = temp_dir();
    write_config(
        &dir,
        r#"
backend = "github"

[github]
owner = "myorg"
repo = "myrepo"
token = "ghp_secret"
"#,
    );

    track_in(&dir)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("github"))
        .stdout(predicate::str::contains("myorg"))
        .stdout(predicate::str::contains("myrepo"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_show_json_with_backend_sections() {
    let dir = temp_dir();
    write_config(
        &dir,
        r#"
backend = "github"

[github]
owner = "org"
repo = "repo"
token = "ghp_tok"
"#,
    );

    let output = track_in(&dir)
        .args(["-o", "json", "config", "show"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // New format: { "config": [{ "key": "...", "value": "...", "source": "..." }, ...] }
    let config = json["config"]
        .as_array()
        .expect("config should be an array");
    let find_entry =
        |key: &str| -> Option<&serde_json::Value> { config.iter().find(|e| e["key"] == key) };
    assert_eq!(find_entry("backend").unwrap()["value"], "github");
    assert_eq!(find_entry("github.owner").unwrap()["value"], "org");
    assert_eq!(find_entry("github.repo").unwrap()["value"], "repo");
    // Token should be hidden
    assert_eq!(
        find_entry("github.token").unwrap()["value"],
        "(set - hidden)"
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// init does not need API
// =============================================================================

#[test]
fn test_init_does_not_need_existing_config() {
    let dir = temp_dir();

    // init should work even without any existing config
    track_in(&dir)
        .args(["init", "--url", "https://yt.test", "--token", "tok"])
        .assert()
        .success();

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// eval status does not need API
// =============================================================================

#[test]
fn test_eval_status_no_api_needed() {
    let dir = temp_dir();

    track_in(&dir)
        .args(["eval", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TRACK_MOCK_DIR"));

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// config clear JSON output
// =============================================================================

#[test]
fn test_config_clear_json_output() {
    let dir = temp_dir();
    write_config(&dir, "backend = \"youtrack\"\ndefault_project = \"PROJ\"\n");

    let output = track_in(&dir)
        .args(["-o", "json", "config", "clear"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["success"], true);

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// --body-file Tests
// =============================================================================

#[test]
fn test_body_file_appears_in_issue_create_help() {
    cargo_bin_cmd!("track")
        .args(["issue", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--body-file"))
        .stdout(predicate::str::contains("stdin"));
}

#[test]
fn test_body_file_appears_in_issue_update_help() {
    cargo_bin_cmd!("track")
        .args(["issue", "update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--body-file"));
}

#[test]
fn test_body_file_appears_in_issue_comment_help() {
    cargo_bin_cmd!("track")
        .args(["issue", "comment", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--body-file"));
}

#[test]
fn test_body_file_appears_in_project_create_help() {
    cargo_bin_cmd!("track")
        .args(["project", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--body-file"));
}

#[test]
fn test_body_file_appears_in_tag_create_help() {
    cargo_bin_cmd!("track")
        .args(["tags", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--body-file"));
}

#[test]
fn test_body_file_appears_in_tag_update_help() {
    cargo_bin_cmd!("track")
        .args(["tags", "update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--body-file"));
}

#[test]
fn test_body_file_appears_in_article_create_help() {
    cargo_bin_cmd!("track")
        .args(["article", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--body-file"));
}

#[test]
fn test_body_file_appears_in_article_update_help() {
    cargo_bin_cmd!("track")
        .args(["article", "update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--body-file"));
}

#[test]
fn test_body_file_appears_in_article_comment_help() {
    cargo_bin_cmd!("track")
        .args(["article", "comment", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--body-file"));
}

#[test]
fn test_content_file_hidden_from_article_help() {
    // --content-file should be hidden (backward compat only)
    cargo_bin_cmd!("track")
        .args(["article", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--content-file").not());
}

#[test]
fn test_body_file_conflicts_with_description() {
    cargo_bin_cmd!("track")
        .args([
            "issue",
            "create",
            "-p",
            "PROJ",
            "-s",
            "Title",
            "-d",
            "inline desc",
            "--body-file",
            "/tmp/desc.md",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn test_body_file_conflicts_with_json_on_update() {
    cargo_bin_cmd!("track")
        .args([
            "issue",
            "update",
            "PROJ-1",
            "--json",
            "{\"summary\":\"test\"}",
            "--body-file",
            "/tmp/desc.md",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn test_comment_requires_message_or_body_file() {
    cargo_bin_cmd!("track")
        .args(["issue", "comment", "PROJ-1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--message").or(predicate::str::contains("--body-file")));
}

#[test]
fn test_body_file_reads_file_via_mock() {
    let dir = temp_dir();
    let scenario = fixtures_path().join("basic-workflow");

    // Write a body file
    let body = dir.join("comment.md");
    fs::write(&body, "Comment from file\n").unwrap();

    // Use mock mode to test the full flow — comment command reads the file
    // and sends it to the mock backend
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args([
            "issue",
            "comment",
            "DEMO-1",
            "--body-file",
            body.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Comment"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_body_file_error_on_missing_file() {
    let dir = temp_dir();
    let scenario = fixtures_path().join("basic-workflow");

    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args([
            "issue",
            "update",
            "DEMO-1",
            "--body-file",
            "/tmp/track-nonexistent-file-xyz.md",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to read"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_body_file_reads_multiline_markdown() {
    let dir = temp_dir();
    let scenario = fixtures_path().join("basic-workflow");

    // Write complex markdown with code blocks, angle brackets, etc.
    let body = dir.join("complex.md");
    fs::write(
        &body,
        "## Overview\n\n```rust\nfn main() -> Result<String> {\n    Ok(\"hello\".into())\n}\n```\n\n- Item 1\n- Item 2\n",
    )
    .unwrap();

    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args([
            "issue",
            "comment",
            "DEMO-1",
            "--body-file",
            body.to_str().unwrap(),
        ])
        .assert()
        .success();

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_body_file_stdin_via_pipe() {
    let dir = temp_dir();
    let scenario = fixtures_path().join("basic-workflow");

    // Test that --body-file - reads from stdin
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args(["issue", "comment", "DEMO-1", "--body-file", "-"])
        .write_stdin("Comment from stdin")
        .assert()
        .success()
        .stdout(predicate::str::contains("Comment"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_body_file_update_satisfies_required_fields() {
    let dir = temp_dir();
    let scenario = fixtures_path().join("basic-workflow");

    let body = dir.join("desc.md");
    fs::write(&body, "Updated description\n").unwrap();

    // --body-file alone should satisfy the "at least one field" requirement
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args([
            "issue",
            "update",
            "DEMO-1",
            "--body-file",
            body.to_str().unwrap(),
        ])
        .assert()
        .success();

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_content_file_backward_compat_still_parses() {
    // --content-file should still be accepted on article commands (hidden alias).
    // We verify the CLI does not reject the flag at parse time.
    // The actual API call may fail (mock may not support update_article),
    // so we only check that the error is NOT a parse/usage error.
    let dir = temp_dir();
    let scenario = fixtures_path().join("basic-workflow");

    let body = dir.join("article.md");
    fs::write(&body, "Article content from file\n").unwrap();

    let output = cargo_bin_cmd!("track")
        .current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args([
            "article",
            "update",
            "DEMO-A-1",
            "--content-file",
            body.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT be a CLI parse error — any API-level error is fine
    assert!(
        !stderr.contains("unrecognized")
            && !stderr.contains("not expected in this context")
            && !stderr.contains("invalid value"),
        "--content-file should be accepted as a hidden flag, got: {}",
        stderr
    );

    let _ = fs::remove_dir_all(&dir);
}
