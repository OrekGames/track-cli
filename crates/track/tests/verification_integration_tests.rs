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
                // Read headers
                let _ = stream.read(&mut buffer);

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
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

/// A fake GitHub API for issue #42 of `o/r`: GET returns it in `initial_state`,
/// PATCH returns it in `final_state`. Returns the server's port and the
/// `METHOD path body` of every request it answered.
fn start_github_issue_server(
    initial_state: &'static str,
    final_state: &'static str,
) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use std::io::{BufRead, BufReader, Read, Write};

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let issue = |state: &str| {
        serde_json::json!({
            "id": 1042, "number": 42, "title": "Ship it", "body": "", "state": state,
            "labels": [], "assignee": null, "assignees": [], "milestone": null,
            "created_at": "2024-01-15T10:30:00Z", "updated_at": "2024-01-15T12:00:00Z",
            "closed_at": null, "user": {"login": "reporter", "id": 2}, "pull_request": null
        })
        .to_string()
    };

    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            let _ = reader.read_line(&mut request_line);
            let mut content_length = 0;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; content_length];
            let _ = reader.read_exact(&mut body);

            let mut parts = request_line.split_whitespace();
            let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let (status, response) = match (method, path) {
                ("GET", "/repos/o/r/issues/42") => ("200 OK", issue(initial_state)),
                ("PATCH", "/repos/o/r/issues/42") => ("200 OK", issue(final_state)),
                _ => ("404 Not Found", r#"{"message":"Not Found"}"#.to_string()),
            };
            recorded.lock().unwrap().push(format!(
                "{method} {path} {}",
                String::from_utf8_lossy(&body)
            ));
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            );
        }
    });

    (port, requests)
}

fn track_github(port: u16) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "github"])
        .env("GITHUB_TOKEN", "test-token")
        .env("GITHUB_OWNER", "o")
        .env("GITHUB_REPO", "r")
        .env("GITHUB_API_URL", format!("http://127.0.0.1:{port}"))
        .env_remove("TRACKER_URL")
        .env_remove("TRACKER_TOKEN")
        .env_remove("TRACKER_CONFIG");
    cmd
}

/// Without --state, start/complete use the backend's own state names, so the
/// update verifies cleanly instead of warning that "Done" came back "closed".
#[test]
fn test_github_start_and_complete_default_to_open_and_closed() {
    for (action, initial, expected) in [("complete", "open", "closed"), ("start", "closed", "open")]
    {
        let (port, requests) = start_github_issue_server(initial, expected);

        track_github(port)
            .args(["issue", action, "42"])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("Status={expected}")))
            .stderr(predicate::str::contains("Warning").not());

        let patch = requests
            .lock()
            .unwrap()
            .iter()
            .find(|request| request.starts_with("PATCH "))
            .cloned()
            .unwrap_or_else(|| panic!("{action} sent no PATCH"));
        let body: serde_json::Value =
            serde_json::from_str(patch.splitn(3, ' ').nth(2).unwrap()).unwrap();
        assert_eq!(body["state"], expected, "{action}: {patch}");
    }
}

/// State names are compared case-insensitively, as every backend matches them.
#[test]
fn test_state_verification_ignores_case() {
    let (port, _requests) = start_github_issue_server("open", "closed");

    track_github(port)
        .args(["issue", "complete", "42", "--state", "Closed"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Warning").not());
}

/// A transition that did not happen still warns.
#[test]
fn test_state_verification_warns_when_state_did_not_change() {
    let (port, _requests) = start_github_issue_server("open", "open");

    track_github(port)
        .args(["issue", "complete", "42"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Field 'Status': expected 'closed', got 'open'",
        ));
}
