use predicates::prelude::*;
use serde_json::json;

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

mod common;
use common::{TestDir, canonical_temp_dir, track, track_in};

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
                "HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: {}
Connection: close

{}",
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

fn global_config_path(temp_home: &Path) -> PathBuf {
    temp_home.join(".tracker-cli").join(".track.toml")
}

fn assert_init_rejects_url(url: &str) {
    let temp_dir = canonical_temp_dir();
    let config_path = global_config_path(&temp_dir);

    track_in(&temp_dir)
        .args([
            "--format", "json", "init", "--url", url, "--token", "test", "-b", "youtrack",
            "--global",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains(format!("Invalid --url '{url}'"))
                .and(predicate::str::contains(
                    "expected an absolute https:// URL",
                ))
                .and(predicate::str::contains(
                    "Plain http:// is allowed only for localhost",
                )),
        );

    assert!(
        !config_path.exists(),
        "rejected init URL should not create config: {}",
        config_path.display()
    );
}

fn assert_init_rejects_userinfo_url(url: &str) {
    let temp_dir = canonical_temp_dir();
    let config_path = global_config_path(&temp_dir);

    let output = track_in(&temp_dir)
        .args([
            "--format", "json", "init", "--url", url, "--token", "test", "-b", "youtrack",
            "--global",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected userinfo URL to be rejected: {url}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid --url") || stderr.to_ascii_lowercase().contains("userinfo"),
        "expected invalid-url / userinfo phase, got:\n{stderr}"
    );
    assert!(
        !stderr.contains('@') && !stderr.contains(url),
        "stderr must not echo userinfo or the raw URL:\n{stderr}"
    );
    assert!(
        !config_path.exists(),
        "rejected init URL should not create config: {}",
        config_path.display()
    );
}

fn assert_init_allows_url(url: &str) {
    let temp_dir = canonical_temp_dir();
    let config_path = global_config_path(&temp_dir);

    let output = track_in(&temp_dir)
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
}

#[test]
fn test_missing_config() {
    let temp_home = canonical_temp_dir();

    track_in(&temp_home)
        .args(["issue", "get", "PROJ-1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("URL not configured"));
}

#[test]
fn test_help_command() {
    track()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("CLI for issue tracking systems"));
}

#[test]
fn test_issue_subcommand_help() {
    track()
        .args(["issue", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Issue operations"));
}

#[test]
fn test_project_subcommand_help() {
    track()
        .args(["project", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Project operations"));
}

#[test]
fn test_version() {
    track()
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
        assert_init_rejects_url(url);
    }

    for url in [
        "http://localhost:token@example.com",
        "http://127.0.0.1:token@example.com",
    ] {
        assert_init_rejects_userinfo_url(url);
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
    let temp_dir = canonical_temp_dir();
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
    let config_path = std::fs::canonicalize(&config_path).unwrap();

    let output = track_in(&temp_dir)
        .args(["--config"])
        .arg(&config_path)
        .args(["--format", "json", "project", "list"])
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
}

/// Write a `TRACK_MOCK_DIR` scenario that answers each `(method, JSON body)`
/// pair for any arguments.
fn write_mock_scenario(responses: &[(&str, &str)]) -> TestDir {
    let dir = canonical_temp_dir();
    let responses_dir = dir.join("responses");
    std::fs::create_dir_all(&responses_dir).unwrap();
    let mut manifest = String::new();
    for (method, body) in responses {
        let file = format!("{method}.json");
        std::fs::write(responses_dir.join(&file), body).unwrap();
        manifest.push_str(&format!(
            "[[responses]]\nmethod = \"{method}\"\nfile = \"{file}\"\n\n"
        ));
    }
    std::fs::write(dir.join("manifest.toml"), manifest).unwrap();
    dir
}

const MOCK_ARTICLE: &str = r#"{
    "id": "kb-1",
    "id_readable": "KB-A-1",
    "summary": "Setup Guide",
    "content": null,
    "project": { "id": "0-1", "name": "Demo", "short_name": "DEMO" },
    "parent_article": { "id": "kb-2", "id_readable": "KB-A-2", "summary": null },
    "has_children": false,
    "tags": [],
    "created": "2024-01-05T11:00:00Z",
    "updated": "2024-01-10T09:00:00Z",
    "reporter": null
}"#;

fn stdout_json(args: &[&str], mock_dir: &Path) -> serde_json::Value {
    let output = track_in(mock_dir)
        .env("TRACK_MOCK_DIR", mock_dir)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap_or_else(|e| {
        panic!(
            "stdout of {args:?} is not JSON ({e}):\n{}",
            String::from_utf8_lossy(&output)
        )
    })
}

#[test]
fn article_move_json_output_is_only_the_article() {
    let mock_dir = write_mock_scenario(&[("move_article", MOCK_ARTICLE)]);

    let json = stdout_json(
        &[
            "-o", "json", "article", "move", "KB-A-1", "--parent", "KB-A-2",
        ],
        &mock_dir,
    );
    assert_eq!(json["id_readable"], "KB-A-1");
}

#[test]
fn article_delete_json_output_is_json() {
    let mock_dir = write_mock_scenario(&[("delete_article", "null")]);

    let json = stdout_json(&["-o", "json", "article", "delete", "KB-A-1"], &mock_dir);
    assert_eq!(json["success"], true);
}

// These tests require a mock server or real YouTrack instance
// They're commented out but show how to test with environment variables

/*
#[test]
fn test_json_output_format() {
    track()
        .args(["--format", "json", "issue", "get", "PROJ-1"])
        .env("YOUTRACK_TOKEN", "test-token")
        .env("YOUTRACK_URL", "https://test.youtrack.cloud")
        .assert()
        .stdout(predicate::str::starts_with("{"));
}

#[test]
fn test_search_issues() {
    track()
        .args(["issue", "search", "project: Test", "--limit", "10"])
        .env("YOUTRACK_TOKEN", "test-token")
        .env("YOUTRACK_URL", "https://test.youtrack.cloud")
        .assert()
        .success();
}
*/
