use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::json;
use std::sync::atomic::{AtomicU16, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Helper function to get an available port with atomic counter to avoid conflicts
static PORT_COUNTER: AtomicU16 = AtomicU16::new(51000);

fn get_available_port() -> u16 {
    PORT_COUNTER.fetch_add(1, Ordering::SeqCst)
}

// Helper to create a simple mock server
fn start_mock_server(port: u16, response_body: String) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let bind_addr = format!("127.0.0.1:{}", port);
        let listener = match TcpListener::bind(&bind_addr) {
            Ok(l) => l,
            Err(_) => return, // Port already in use, exit gracefully
        };

        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                let mut buffer = [0; 4096];
                if stream.read(&mut buffer).is_ok() {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                // Exit after first request
                break;
            }
        }
    })
}

fn create_temp_dir() -> std::path::PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("track-test-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_missing_config() {
    cargo_bin_cmd!("track")
        .args(["issue", "get", "PROJ-1"])
        .env_remove("TRACKER_URL")
        .env_remove("TRACKER_TOKEN")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("URL not configured"));
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
fn test_config_file_is_used_for_defaults() {
    let temp_dir = create_temp_dir();
    let config_path = temp_dir.join("config.toml");

    let port = get_available_port();
    let url = format!("http://127.0.0.1:{}", port);

    let config_contents = format!("url = \"{}\"\ntoken = \"test-token\"\n", url);
    std::fs::write(&config_path, config_contents).unwrap();

    let mock_response = json!([{
        "id": "0-1",
        "name": "Test Project",
        "shortName": "PROJ",
        "description": "A test project"
    }]);

    let _server = start_mock_server(port, mock_response.to_string());
    thread::sleep(Duration::from_millis(200));

    let output = cargo_bin_cmd!("track")
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "--format",
            "json",
            "project",
            "list",
        ])
        .env_remove("TRACKER_URL")
        .env_remove("TRACKER_TOKEN")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
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
