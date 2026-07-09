//! Integration tests for the config, init, open, and issue-shortcut commands.
//!
//! These test the command handlers extracted during the main.rs refactoring.
//! All tests use temp directories for isolation and don't require a real backend.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

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
        .env_remove("LINEAR_TOKEN")
        .env_remove("LINEAR_API_URL")
        .env_remove("LINEAR_URL")
        .env_remove("LINEAR_DEFAULT_TEAM")
        .env_remove("LINEAR_DEFAULT_PROJECT")
        .env_remove("TRACK_MOCK_DIR");
    cmd
}

/// Write a minimal .track.toml in the given directory.
fn write_config(dir: &Path, content: &str) {
    fs::write(dir.join(".track.toml"), content).unwrap();
}

fn start_one_request_json_server(body: &'static str) -> (thread::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0; 2048];
        let _ = stream.read(&mut buffer);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    (handle, format!("http://{}", addr))
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

fn copy_dir_recursive(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(&source, &target);
        } else {
            fs::copy(&source, &target).unwrap();
        }
    }
}

fn copy_scenario(dir: &Path, name: &str) -> PathBuf {
    let scenario = dir.join(name);
    copy_dir_recursive(&fixtures_path().join(name), &scenario);
    fs::write(scenario.join("call_log.jsonl"), "").unwrap();
    scenario
}

fn add_second_basic_issue(scenario: &Path) {
    fs::copy(
        scenario.join("responses/get_issue_DEMO-1.json"),
        scenario.join("responses/get_issue_DEMO-2.json"),
    )
    .unwrap();

    let mut manifest = fs::OpenOptions::new()
        .append(true)
        .open(scenario.join("manifest.toml"))
        .unwrap();
    writeln!(
        manifest,
        r#"
[[responses]]
method = "get_issue"
file = "get_issue_DEMO-2.json"
[responses.args]
id = "DEMO-2"
"#
    )
    .unwrap();
}

fn write_comments_response(scenario: &Path, count: usize) {
    let comments: Vec<_> = (1..=count)
        .map(|index| {
            serde_json::json!({
                "id": format!("comment-{index}"),
                "text": format!("Comment {index}"),
                "author": null,
                "created": null
            })
        })
        .collect();
    fs::write(
        scenario.join("responses/get_comments_DEMO-1.json"),
        serde_json::to_string(&comments).unwrap(),
    )
    .unwrap();
}

fn mock_call_methods(scenario: &Path) -> Vec<String> {
    let log = fs::read_to_string(scenario.join("call_log.jsonl")).unwrap_or_default();
    log.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .map(|entry| entry["method"].as_str().unwrap().to_string())
        .collect()
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
        .stdout(predicate::str::contains("gitlab.token"))
        .stdout(predicate::str::contains("linear.token"))
        .stdout(predicate::str::contains("linear.default_linear_project"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_config_backend_accepts_linear_alias() {
    let dir = temp_dir();
    write_config(&dir, "backend = \"youtrack\"\n");

    track_in(&dir)
        .args(["config", "backend", "lin"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.join(".track.toml")).unwrap();
    assert!(content.contains("backend = \"linear\""));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_linear_init_writes_backend_specific_config_without_api_validation() {
    let dir = temp_dir();

    track_in(&dir)
        .args([
            "init",
            "-b",
            "linear",
            "--url",
            "https://linear.app/acme",
            "--token",
            "lin-token",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(dir.join(".track.toml")).unwrap();
    assert!(content.contains("backend = \"linear\""));
    assert!(content.contains("[linear]"));
    assert!(content.contains("url = \"https://linear.app/acme\""));
    assert!(content.contains("token = \"lin-token\""));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_comments_all_fetches_multiple_pages() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    write_comments_response(&scenario, 150);

    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args(["issue", "comments", "DEMO-1", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Comments on DEMO-1 (150)"));

    let methods = mock_call_methods(&scenario);
    let get_comments_calls = methods
        .iter()
        .filter(|method| method.as_str() == "get_comments")
        .count();
    assert_eq!(
        get_comments_calls, 2,
        "--all should request comment pages until the final partial page, got methods: {:?}",
        methods
    );

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

    // JSON output should mask the actual value
    let output = track_in(&dir)
        .args(["-o", "json", "config", "get", "token"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["value"], "(set - hidden)");

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
        !dir.join(".gitignore").exists(),
        "init should not create .gitignore when one is not already present"
    );
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
fn test_init_updates_existing_gitignore() {
    let dir = temp_dir();
    fs::write(dir.join(".gitignore"), "target/\n").unwrap();

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
        .stdout(predicate::str::contains(".gitignore"));

    let content = fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(content.contains("target/"));
    assert!(content.lines().any(|line| line == ".track.toml"));
    assert!(content.lines().any(|line| line == ".tracker-cache/"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_does_not_duplicate_gitignore_entries() {
    let dir = temp_dir();
    fs::write(dir.join(".gitignore"), ".track.toml\n.tracker-cache/\n").unwrap();

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
        .stdout(predicate::str::contains(".gitignore").not());

    let content = fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert_eq!(
        content
            .lines()
            .filter(|line| *line == ".track.toml")
            .count(),
        1
    );
    assert_eq!(
        content
            .lines()
            .filter(|line| line.trim_end_matches('/') == ".tracker-cache")
            .count(),
        1
    );

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

#[test]
fn test_init_github_requires_project() {
    let dir = temp_dir();

    track_in(&dir)
        .args([
            "init",
            "--url",
            "https://api.github.com",
            "--token",
            "tok",
            "-b",
            "github",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("GitHub init requires --project"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_github_writes_backend_specific_config() {
    let dir = temp_dir();
    let (server, url) = start_one_request_json_server(
        r#"{"id":1,"name":"repo","full_name":"owner/repo","description":null,"owner":{"login":"owner","id":2}}"#,
    );

    track_in(&dir)
        .args([
            "init",
            "--url",
            &url,
            "--token",
            "ghp-token",
            "-b",
            "github",
            "--project",
            "owner/repo",
        ])
        .assert()
        .success();
    server.join().unwrap();

    let content = fs::read_to_string(dir.join(".track.toml")).unwrap();
    assert!(content.contains("backend = \"github\""));
    assert!(content.contains("[github]"));
    assert!(content.contains("token = \"ghp-token\""));
    assert!(content.contains("owner = \"owner\""));
    assert!(content.contains("repo = \"repo\""));
    assert!(content.contains(&format!("api_url = \"{}\"", url)));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_gitlab_requires_project() {
    let dir = temp_dir();

    track_in(&dir)
        .args([
            "init",
            "--url",
            "https://gitlab.com/api/v4",
            "--token",
            "tok",
            "-b",
            "gitlab",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("GitLab init requires --project"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_init_gitlab_writes_backend_specific_config() {
    let dir = temp_dir();
    let (server, url) = start_one_request_json_server(
        r#"{"id":42,"name":"project","name_with_namespace":"group / project","path":"project","path_with_namespace":"group/project","description":null,"web_url":"https://gitlab.example/group/project"}"#,
    );

    track_in(&dir)
        .args([
            "init",
            "--url",
            &url,
            "--token",
            "glpat-token",
            "-b",
            "gitlab",
            "--project",
            "group/project",
        ])
        .assert()
        .success();
    server.join().unwrap();

    let content = fs::read_to_string(dir.join(".track.toml")).unwrap();
    assert!(content.contains("backend = \"gitlab\""));
    assert!(content.contains("[gitlab]"));
    assert!(content.contains("url = \""));
    assert!(content.contains("token = \"glpat-token\""));
    assert!(content.contains("project_id = \"42\""));

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
fn test_issue_update_validate_dry_run_does_not_update_single_issue() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args([
            "issue",
            "update",
            "DEMO-1",
            "--summary",
            "Dry run summary",
            "--description",
            "Dry run description",
            "--state",
            "Done",
            "--tag",
            "triage",
            "--parent",
            "DEMO-99",
            "--validate",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Validation passed"));

    let methods = mock_call_methods(&scenario);
    assert!(
        !methods.iter().any(|method| method == "update_issue"),
        "dry-run update must not call update_issue, got methods: {:?}",
        methods
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_update_validate_dry_run_does_not_update_batch() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    add_second_basic_issue(&scenario);

    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args([
            "issue",
            "update",
            "DEMO-1,DEMO-2",
            "--summary",
            "Batch dry run summary",
            "--state",
            "Done",
            "--tag",
            "triage",
            "--parent",
            "DEMO-99",
            "--validate",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("2 issues validated"));

    let methods = mock_call_methods(&scenario);
    assert!(
        !methods.iter().any(|method| method == "update_issue"),
        "batch dry-run update must not call update_issue, got methods: {:?}",
        methods
    );

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

// =============================================================================
// issue inspect
// =============================================================================

/// Build a track command wired to a mock scenario.
fn track_mock(dir: &Path, scenario: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"]);
    cmd
}

fn parse_json_stdout(output: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON ({e}): {stdout}"))
}

#[test]
fn test_issue_inspect_positional_ids_json_shape() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "inspect",
            "DEMO-1",
            "--include",
            "comments,links",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");

    let report = parse_json_stdout(&output);
    assert_eq!(report["total"], 1);
    assert_eq!(report["succeeded"], 1);
    assert_eq!(report["failed"], 0);
    assert!(report["errors"].as_array().unwrap().is_empty());
    // query_total is query-mode only
    assert!(report.get("query_total").is_none());

    let issue = &report["issues"][0];
    assert_eq!(issue["id_readable"], "DEMO-1");
    assert_eq!(issue["summary"], "Implement user authentication");
    assert_eq!(issue["comments"].as_array().unwrap().len(), 1);
    assert_eq!(issue["links"].as_array().unwrap().len(), 1);
    // subtasks/history were not requested
    assert!(issue.get("subtasks").is_none());
    assert!(issue.get("history").is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_ids_file() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    add_second_basic_issue(&scenario);
    // add_second_basic_issue copies DEMO-1's response verbatim; give the
    // DEMO-2 mapping its own readable ID so we can assert on it.
    let demo2 = scenario.join("responses/get_issue_DEMO-2.json");
    let patched = fs::read_to_string(&demo2)
        .unwrap()
        .replace("\"DEMO-1\"", "\"DEMO-2\"");
    fs::write(&demo2, patched).unwrap();

    let ids_file = dir.join("ids.txt");
    fs::write(&ids_file, "DEMO-1\n\n# comment line\n  DEMO-2  \nDEMO-1\n").unwrap();

    let output = track_mock(&dir, &scenario)
        .args(["issue", "inspect", "--ids"])
        .arg(&ids_file)
        .args(["-o", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");

    let report = parse_json_stdout(&output);
    // DEMO-1 duplicate is deduplicated; blank/comment lines skipped
    assert_eq!(report["total"], 2);
    assert_eq!(report["succeeded"], 2);
    assert_eq!(report["failed"], 0);
    assert_eq!(report["issues"][0]["id_readable"], "DEMO-1");
    assert_eq!(report["issues"][1]["id_readable"], "DEMO-2");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_positional_ids_combine_with_ids_file() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    add_second_basic_issue(&scenario);
    let demo2 = scenario.join("responses/get_issue_DEMO-2.json");
    let patched = fs::read_to_string(&demo2)
        .unwrap()
        .replace("\"DEMO-1\"", "\"DEMO-2\"");
    fs::write(&demo2, patched).unwrap();

    let ids_file = dir.join("ids.txt");
    fs::write(&ids_file, "DEMO-2\nDEMO-1\n").unwrap();

    let output = track_mock(&dir, &scenario)
        .args(["issue", "inspect", "DEMO-1", "--ids"])
        .arg(&ids_file)
        .args(["-o", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "positional IDs must combine with --ids: {output:?}"
    );

    let report = parse_json_stdout(&output);
    // Positional IDs come first; the file's duplicate DEMO-1 is deduplicated
    assert_eq!(report["total"], 2);
    assert_eq!(report["succeeded"], 2);
    assert_eq!(report["issues"][0]["id_readable"], "DEMO-1");
    assert_eq!(report["issues"][1]["id_readable"], "DEMO-2");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_ids_stdin() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args(["issue", "inspect", "--ids", "-", "-o", "json"])
        .write_stdin("DEMO-1\n")
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");

    let report = parse_json_stdout(&output);
    assert_eq!(report["total"], 1);
    assert_eq!(report["issues"][0]["id_readable"], "DEMO-1");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_query_uses_search_not_get() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "inspect",
            "--query",
            "project: DEMO",
            "--limit",
            "10",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");

    let report = parse_json_stdout(&output);
    assert_eq!(report["total"], 2);
    assert_eq!(report["succeeded"], 2);
    assert_eq!(report["issues"][0]["id_readable"], "DEMO-1");
    assert_eq!(report["issues"][1]["id_readable"], "DEMO-2");
    // Backend did not report a match count -> no query_total field
    assert!(report.get("query_total").is_none());

    // Search results already carry full issues: no per-issue get_issue calls
    let methods = mock_call_methods(&scenario);
    assert!(methods.iter().any(|m| m == "search_issues"));
    assert!(
        !methods.iter().any(|m| m == "get_issue"),
        "query mode must not re-fetch issues via get_issue, got: {methods:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_query_total_reports_backend_match_count() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    // Wrap the search fixture in a SearchResult shape whose total exceeds the
    // returned page, simulating a truncated query.
    let search = scenario.join("responses/search_issues_DEMO.json");
    let items: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&search).unwrap()).unwrap();
    fs::write(
        &search,
        serde_json::json!({ "items": items, "total": 42 }).to_string(),
    )
    .unwrap();

    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "inspect",
            "--query",
            "project: DEMO",
            "--limit",
            "2",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");

    let report = parse_json_stdout(&output);
    // total stays "results in this report"; query_total carries the backend count
    assert_eq!(report["total"], 2);
    assert_eq!(report["query_total"], 42);

    // Text mode surfaces the truncation
    track_mock(&dir, &scenario)
        .args([
            "issue",
            "inspect",
            "--query",
            "project: DEMO",
            "--limit",
            "2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Query matched 42 issues"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_partial_failure_exits_zero() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args(["issue", "inspect", "DEMO-1,NOTFOUND-999", "-o", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "partial failure must exit 0 without --strict: {output:?}"
    );

    let report = parse_json_stdout(&output);
    assert_eq!(report["total"], 2);
    assert_eq!(report["succeeded"], 1);
    assert_eq!(report["failed"], 1);
    assert_eq!(report["issues"].as_array().unwrap().len(), 1);
    assert_eq!(report["issues"][0]["id_readable"], "DEMO-1");
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["id"], "NOTFOUND-999");
    assert!(
        errors[0]["error"].as_str().unwrap().contains("not found")
            || errors[0]["error"].as_str().unwrap().contains("404"),
        "error should describe the failure: {}",
        errors[0]["error"]
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_strict_reports_all_then_fails() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "inspect",
            "DEMO-1,NOTFOUND-999",
            "--strict",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "--strict with a failed issue must exit non-zero"
    );

    // Full report is still emitted before the strict failure
    let report = parse_json_stdout(&output);
    assert_eq!(report["succeeded"], 1);
    assert_eq!(report["failed"], 1);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("1 of 2 issues failed inspection"),
        "stderr should explain the strict failure: {stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_strict_succeeds_when_all_pass() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    track_mock(&dir, &scenario)
        .args(["issue", "inspect", "DEMO-1", "--strict", "-o", "json"])
        .assert()
        .success();

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_jsonl_one_line_per_result() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "inspect",
            "DEMO-1,NOTFOUND-999",
            "--include",
            "comments",
            "--jsonl",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        2,
        "expected one JSONL line per issue: {stdout}"
    );

    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["success"], true);
    assert_eq!(first["id_readable"], "DEMO-1");
    assert_eq!(first["comments"].as_array().unwrap().len(), 1);

    let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(second["success"], false);
    assert_eq!(second["id"], "NOTFOUND-999");
    assert!(second["error"].is_string());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_subtasks_derived_from_links() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "inspect",
            "DEMO-1",
            "--include",
            "subtasks",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");

    let report = parse_json_stdout(&output);
    let issue = &report["issues"][0];
    let subtasks = issue["subtasks"].as_array().unwrap();
    assert_eq!(subtasks.len(), 1);
    assert_eq!(subtasks[0]["link_type"]["name"], "Subtask");
    // links itself was not requested, only the subtasks view
    assert!(issue.get("links").is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_subtasks_honor_link_mappings() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    // Rename the hierarchy link type to something the default name heuristic
    // cannot recognize.
    let links = scenario.join("responses/get_issue_links_DEMO-1.json");
    let patched = fs::read_to_string(&links)
        .unwrap()
        .replace("\"Subtask\"", "\"Blocks Chain\"")
        .replace("\"is parent for\"", "\"blocks\"")
        .replace("\"is subtask of\"", "\"is blocked by\"");
    fs::write(&links, patched).unwrap();

    let inspect_args = [
        "issue",
        "inspect",
        "DEMO-1",
        "--include",
        "subtasks",
        "-o",
        "json",
    ];

    // Without a mapping the renamed type is not classified as a subtask link
    let output = track_mock(&dir, &scenario)
        .args(inspect_args)
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");
    let report = parse_json_stdout(&output);
    assert!(
        report["issues"][0]["subtasks"]
            .as_array()
            .unwrap()
            .is_empty(),
        "renamed link type must not match without a mapping: {report}"
    );

    // Mapping the canonical subtask keyword to the custom name classifies it
    fs::write(
        dir.join(".track.toml"),
        "backend = \"youtrack\"\n[youtrack.link_mappings]\nsubtask = \"Blocks Chain\"\n",
    )
    .unwrap();
    let output = track_mock(&dir, &scenario)
        .args(inspect_args)
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");
    let report = parse_json_stdout(&output);
    let subtasks = report["issues"][0]["subtasks"].as_array().unwrap();
    assert_eq!(
        subtasks.len(),
        1,
        "mapped link type must classify: {report}"
    );
    assert_eq!(subtasks[0]["link_type"]["name"], "Blocks Chain");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_unsupported_include_is_warning_not_failure() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    // basic-workflow has no get_issue_history mapping -> the include fails,
    // but the issue itself must still succeed with a structured warning.
    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "inspect",
            "DEMO-1",
            "--include",
            "history",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");

    let report = parse_json_stdout(&output);
    assert_eq!(report["succeeded"], 1);
    assert_eq!(report["failed"], 0);
    let issue = &report["issues"][0];
    assert!(issue.get("history").is_none());
    let warnings = issue["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["include"], "history");
    assert!(warnings[0]["message"].is_string());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_repeated_include_flags() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "inspect",
            "DEMO-1",
            "--include",
            "comments",
            "--include",
            "links",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "expected success: {output:?}");

    let report = parse_json_stdout(&output);
    let issue = &report["issues"][0];
    assert!(issue["comments"].is_array());
    assert!(issue["links"].is_array());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_ix_alias_and_text_output() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    track_mock(&dir, &scenario)
        .args(["i", "ix", "DEMO-1,NOTFOUND-999", "--include", "comments"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Inspected 2 issues: 1 succeeded, 1 failed",
        ))
        .stdout(predicate::str::contains("DEMO-1"))
        .stdout(predicate::str::contains("comments: 1"))
        .stdout(predicate::str::contains("NOTFOUND-999"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_no_input_mode_errors() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args(["issue", "inspect"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No issue IDs given"),
        "should explain input modes: {stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_inspect_unknown_include_errors() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let output = track_mock(&dir, &scenario)
        .args(["issue", "inspect", "DEMO-1", "--include", "attachments"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unknown --include value 'attachments'"),
        "should reject unknown include: {stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// issue start/complete state field resolution (issue #308)
// =============================================================================

/// Read the full call log entries (method + args) for a scenario.
fn mock_call_entries(scenario: &Path) -> Vec<serde_json::Value> {
    let log = fs::read_to_string(scenario.join("call_log.jsonl")).unwrap_or_default();
    log.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect()
}

/// Collect (issue id, custom_fields summary) pairs for all update_issue calls.
fn update_issue_calls(scenario: &Path) -> Vec<(String, String)> {
    mock_call_entries(scenario)
        .iter()
        .filter(|entry| entry["method"] == "update_issue")
        .map(|entry| {
            (
                entry["args"]["id"].as_str().unwrap_or_default().to_string(),
                entry["args"]["custom_fields"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

#[test]
fn test_issue_complete_resolves_state_field_from_schema() {
    // Issue #308: a project whose workflow field is named "State" must not
    // receive a hardcoded "Stage" transition.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "state-field-resolution");

    track_mock(&dir, &scenario)
        .args(["issue", "complete", "ALPHA-1", "--state", "Done"])
        .assert()
        .success()
        .stdout(predicate::str::contains("completed"))
        .stdout(predicate::str::contains("State=Done"));

    assert_eq!(
        update_issue_calls(&scenario),
        vec![("ALPHA-1".to_string(), "State=Done".to_string())]
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_complete_resolves_stage_field_from_schema() {
    // A project whose workflow field is named "Stage" still transitions via "Stage".
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "state-field-resolution");

    track_mock(&dir, &scenario)
        .args(["issue", "complete", "BETA-1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("completed"))
        .stdout(predicate::str::contains("Stage=Done"));

    assert_eq!(
        update_issue_calls(&scenario),
        vec![("BETA-1".to_string(), "Stage=Done".to_string())]
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_complete_explicit_field_skips_schema_lookup() {
    // An explicit --field is honored verbatim without any schema resolution.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "state-field-resolution");

    track_mock(&dir, &scenario)
        .args(["issue", "complete", "ALPHA-1", "--field", "Stage"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Stage=Done"));

    let methods = mock_call_methods(&scenario);
    assert!(
        !methods
            .iter()
            .any(|m| m == "get_project_custom_fields" || m == "get_issue"),
        "explicit --field must not trigger schema resolution, got methods: {:?}",
        methods
    );
    assert_eq!(
        update_issue_calls(&scenario),
        vec![("ALPHA-1".to_string(), "Stage=Done".to_string())]
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_complete_explicit_non_state_field_is_enum_update() {
    // A --field name outside State/Stage/Status stays a plain enum update.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "state-field-resolution");

    track_mock(&dir, &scenario)
        .args(["issue", "complete", "ALPHA-1", "--field", "Kanban State"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Kanban State=Done"));

    assert_eq!(
        update_issue_calls(&scenario),
        vec![("ALPHA-1".to_string(), "Kanban State=Done".to_string())]
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_complete_batch_resolves_per_project_with_cached_schema() {
    // A batch spanning two projects resolves each issue's own state field and
    // fetches each project's schema exactly once.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "state-field-resolution");

    track_mock(&dir, &scenario)
        .args(["issue", "complete", "ALPHA-1,BETA-1,ALPHA-2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("3 issues completed"));

    assert_eq!(
        update_issue_calls(&scenario),
        vec![
            ("ALPHA-1".to_string(), "State=Done".to_string()),
            ("BETA-1".to_string(), "Stage=Done".to_string()),
            ("ALPHA-2".to_string(), "State=Done".to_string()),
        ]
    );

    let methods = mock_call_methods(&scenario);
    let schema_lookups = methods
        .iter()
        .filter(|m| m.as_str() == "get_project_custom_fields")
        .count();
    assert_eq!(
        schema_lookups, 2,
        "schema lookups must be cached per project in a batch, got methods: {:?}",
        methods
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_issue_start_resolves_state_field_from_schema() {
    // `issue start` shares the transition path and must resolve the field too.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "state-field-resolution");

    track_mock(&dir, &scenario)
        .args(["issue", "start", "BETA-1", "--state", "Develop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("started"))
        .stdout(predicate::str::contains("Stage=Develop"));

    let _ = fs::remove_dir_all(&dir);
}
