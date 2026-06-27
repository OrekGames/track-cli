use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::json;

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Helper to create a simple mock server
fn start_mock_server(response_body: String) -> (u16, thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let handle = thread::spawn(move || {
        if let Some(mut stream) = listener.incoming().flatten().next() {
            let mut buffer = [0; 4096];
            // Read headers
            let _ = stream.read(&mut buffer);

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();

            // Draining the request body before closing is important on Windows
            // to avoid "Connection forcibly closed by remote host" errors (RST).
            let _ = stream.shutdown(std::net::Shutdown::Write);
            let mut discard = [0; 1024];
            while let Ok(n) = stream.read(&mut discard) {
                if n == 0 {
                    break;
                }
            }
        }
    });

    (port, handle)
}

fn create_temp_dir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("track-test-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn track_with_home(temp_home: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(temp_home)
        .env("HOME", temp_home)
        .env("USERPROFILE", temp_home)
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

fn global_config_path(temp_home: &Path) -> PathBuf {
    temp_home.join(".tracker-cli").join(".track.toml")
}

fn assert_init_rejects_url(url: &str, expected_message: &str) {
    let temp_dir = create_temp_dir();
    let config_path = global_config_path(&temp_dir);

    track_with_home(&temp_dir)
        .args([
            "--format", "json", "init", "--url", url, "--token", "test", "-b", "youtrack",
            "--global",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(expected_message));

    assert!(
        !config_path.exists(),
        "rejected init URL should not create config: {}",
        config_path.display()
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

fn assert_init_allows_url(url: &str) {
    let temp_dir = create_temp_dir();
    let config_path = global_config_path(&temp_dir);

    let output = track_with_home(&temp_dir)
        .args([
            "--format", "json", "init", "--url", url, "--token", "test", "-b", "youtrack",
            "--global",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["config_path"], config_path.display().to_string());

    let config = std::fs::read_to_string(&config_path).unwrap();
    assert!(config.contains("backend = \"youtrack\""));
    assert!(config.contains(&format!("url = \"{url}\"")));
    assert!(config.contains("token = \"test\""));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_missing_config() {
    let temp_home = create_temp_dir();

    track_with_home(&temp_home)
        .args(["issue", "get", "PROJ-1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("URL not configured"));

    let _ = std::fs::remove_dir_all(&temp_home);
}

#[test]
fn test_help_command() {
    cargo_bin_cmd!("track")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("CLI for issue tracking systems"));
}

#[test]
fn test_issue_subcommand_help() {
    cargo_bin_cmd!("track")
        .args(["issue", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Issue operations"));
}

#[test]
fn test_project_subcommand_help() {
    cargo_bin_cmd!("track")
        .args(["project", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Project operations"));
}

#[test]
fn test_version() {
    cargo_bin_cmd!("track")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_init_enforces_https_for_remote_urls() {
    for url in [
        "http://example.com",
        "http://localhost.evil.com",
        "http://127.0.0.1.example.com",
    ] {
        assert_init_rejects_url(url, "Insecure URL");
    }

    for url in [
        "http://localhost:token@example.com",
        "http://127.0.0.1:token@example.com",
    ] {
        assert_init_rejects_url(url, "userinfo is not allowed");
    }

    for url in [
        "http://127.0.0.1",
        "http://127.0.0.1:8080",
        "http://localhost",
        "http://[::1]",
        "http://[::1]:8080",
        "https://example.com",
    ] {
        assert_init_allows_url(url);
    }
}

#[test]
fn test_config_file_is_used_for_defaults() {
    let temp_dir = create_temp_dir();
    let config_path = temp_dir.join("config.toml");

    let mock_response = json!([{
        "id": "0-1",
        "name": "Test Project",
        "shortName": "PROJ",
        "description": "A test project"
    }]);

    let (port, _server) = start_mock_server(mock_response.to_string());

    let url = format!("http://127.0.0.1:{}", port);
    let config_contents = format!("url = \"{}\"\ntoken = \"test-token\"\n", url);
    std::fs::write(&config_path, config_contents).unwrap();

    let output = cargo_bin_cmd!("track")
        .args(["--config"])
        .arg(&config_path)
        .args(["--format", "json", "project", "list"])
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
        .env_remove("TRACK_MOCK_DIR")
        .env("HOME", &temp_dir)
        .env("USERPROFILE", &temp_dir)
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(json.is_array());
    // The output uses snake_case from tracker_core models
    assert_eq!(json[0]["short_name"], "PROJ");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// These tests require a mock server or real YouTrack instance
// They're commented out but show how to test with environment variables

/*
#[test]
fn test_json_output_format() {
    cargo_bin_cmd!("track")
        .args(["--format", "json", "issue", "get", "PROJ-1"])
        .env("YOUTRACK_TOKEN", "test-token")
        .env("YOUTRACK_URL", "https://test.youtrack.cloud")
        .assert()
        .stdout(predicate::str::starts_with("{"));
}

#[test]
fn test_search_issues() {
    cargo_bin_cmd!("track")
        .args(["issue", "search", "project: Test", "--limit", "10"])
        .env("YOUTRACK_TOKEN", "test-token")
        .env("YOUTRACK_URL", "https://test.youtrack.cloud")
        .assert()
        .success();
}
*/
