use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

/// Helper to create a mock server that handles multiple sequential requests.
fn start_mock_server_multi(responses: Vec<String>) -> (thread::JoinHandle<()>, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();

    let handle = thread::spawn(move || {
        use std::io::{Read, Write};
        for body in &responses {
            if let Some(mut stream) = listener.incoming().flatten().next() {
                let mut buffer = [0; 4096];
                if stream.read(&mut buffer).is_ok() {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        }
    });

    (handle, port)
}

#[test]
fn test_issue_update_verification_warning() {
    // 1. Get issue (for validation)
    // 2. Update issue (returns old state to trigger warning)
    let issue_json = serde_json::json!({
        "id": "1-1",
        "idReadable": "PROJ-123",
        "summary": "Old Summary",
        "project": {
            "id": "0-1",
            "shortName": "PROJ"
        },
        "customFields": [],
        "tags": [],
        "created": 1640000000000i64,
        "updated": 1640000000000i64
    })
    .to_string();

    let (_server, port) = start_mock_server_multi(vec![issue_json.clone(), issue_json]);
    thread::sleep(Duration::from_millis(100));

    cargo_bin_cmd!("track")
        .args(["issue", "update", "PROJ-123", "--summary", "New Summary"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .success()
        .stderr(predicate::str::contains("⚠ Warning:"))
        .stderr(predicate::str::contains(
            "Summary: expected 'New Summary', got 'Old Summary'",
        ));
}

#[test]
fn test_issue_create_verification_warning() {
    // 1. Resolve project PROJ -> returns project
    // 2. Create issue -> returns mismatched summary
    let project_json = serde_json::json!([{
        "id": "0-1",
        "name": "Test Project",
        "shortName": "PROJ"
    }])
    .to_string();

    let issue_json = serde_json::json!({
        "id": "1-2",
        "idReadable": "PROJ-124",
        "summary": "Mismatched Summary",
        "project": {
            "id": "0-1",
            "shortName": "PROJ"
        },
        "customFields": [],
        "tags": [],
        "created": 1640000000000i64,
        "updated": 1640000000000i64
    })
    .to_string();

    let (_server, port) = start_mock_server_multi(vec![project_json, issue_json]);
    thread::sleep(Duration::from_millis(100));

    cargo_bin_cmd!("track")
        .args([
            "issue",
            "create",
            "--project",
            "PROJ",
            "--summary",
            "Original Summary",
        ])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .success()
        .stderr(predicate::str::contains("⚠ Warning:"))
        .stderr(predicate::str::contains(
            "Summary: expected 'Original Summary', got 'Mismatched Summary'",
        ));
}

#[test]
fn test_issue_update_verbose_diff() {
    // 1. Get issue (for old state) -> returns "Old Summary"
    // 2. Update issue -> returns "New Summary"
    let old_issue = serde_json::json!({
        "id": "1-1",
        "idReadable": "PROJ-123",
        "summary": "Old Summary",
        "project": { "id": "0-1", "shortName": "PROJ" },
        "customFields": [],
        "tags": [],
        "created": 1640000000000i64,
        "updated": 1640000000000i64
    })
    .to_string();

    let new_issue = serde_json::json!({
        "id": "1-1",
        "idReadable": "PROJ-123",
        "summary": "New Summary",
        "project": { "id": "0-1", "shortName": "PROJ" },
        "customFields": [],
        "tags": [],
        "created": 1640000000000i64,
        "updated": 1640000000001i64
    })
    .to_string();

    let (_server, port) = start_mock_server_multi(vec![old_issue, new_issue.clone(), new_issue]);
    thread::sleep(Duration::from_millis(100));

    cargo_bin_cmd!("track")
        .args([
            "issue",
            "update",
            "PROJ-123",
            "--summary",
            "New Summary",
            "--verbose",
        ])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .success()
        .stdout(predicate::str::contains("--- Change Summary ---"))
        .stdout(predicate::str::contains(
            "Summary: Old Summary -> New Summary",
        ));
}
