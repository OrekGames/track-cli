use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

// Helper to create a mock server that returns the bound port
fn start_mock_server(response_body: String) -> (thread::JoinHandle<()>, u16) {
    // Bind to port 0 to let the OS assign an available port
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();

    let handle = thread::spawn(move || {
        use std::io::{Read, Write};

        // Accept multiple connections (some endpoints chain count + search)
        listener
            .set_nonblocking(false)
            .expect("Failed to set blocking");
        let timeout = Duration::from_secs(3);
        for stream in listener.incoming().flatten().take(3) {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(timeout));
            let mut buffer = [0; 4096];
            if stream.read(&mut buffer).is_ok() {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        }
    });

    (handle, port)
}

/// Helper to create a mock server that handles multiple sequential requests.
/// Each element in `responses` is served in order; extra connections are ignored.
fn start_mock_server_multi(responses: Vec<String>) -> (thread::JoinHandle<()>, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();

    let handle = thread::spawn(move || {
        use std::io::{Read, Write};

        listener
            .set_nonblocking(false)
            .expect("Failed to set blocking");

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
fn test_issue_get_with_json_output() {
    let mock_response = serde_json::json!({
        "id": "2-45",
        "idReadable": "PROJ-123",
        "summary": "Test issue",
        "description": "Test description",
        "project": {
            "id": "0-1",
            "name": "Test Project",
            "shortName": "PROJ"
        },
        "customFields": [],
        "tags": [],
        "created": 1640000000000i64,
        "updated": 1640000000000i64
    });

    let (_server, port) = start_mock_server(mock_response.to_string());
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args(["--format", "json", "issue", "get", "PROJ-123"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();
    // Output uses snake_case from tracker_core models
    assert_eq!(json["id_readable"], "PROJ-123");
    assert_eq!(json["summary"], "Test issue");
}

#[test]
fn test_issue_get_with_text_output() {
    let mock_response = serde_json::json!({
        "id": "2-45",
        "idReadable": "PROJ-456",
        "summary": "Another test issue",
        "project": {
            "id": "0-1",
            "shortName": "PROJ"
        },
        "customFields": [],
        "tags": [],
        "created": 1640000000000i64,
        "updated": 1640000000000i64
    });

    let (_server, port) = start_mock_server(mock_response.to_string());
    thread::sleep(Duration::from_millis(50));

    cargo_bin_cmd!("track")
        .args(["issue", "get", "PROJ-456"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(predicate::str::contains("PROJ-456"))
        .stdout(predicate::str::contains("Another test issue"));
}

#[test]
fn test_issue_search_with_results() {
    // First request: count endpoint response
    let count_response = serde_json::json!({ "count": 2 }).to_string();

    // Second request: search endpoint response
    let search_response = serde_json::json!([
        {
            "id": "2-1",
            "idReadable": "PROJ-1",
            "summary": "First issue",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [],
            "tags": [],
            "created": 1640000000000i64,
            "updated": 1640000000000i64
        },
        {
            "id": "2-2",
            "idReadable": "PROJ-2",
            "summary": "Second issue",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [],
            "tags": [],
            "created": 1640000000000i64,
            "updated": 1640000000000i64
        }
    ])
    .to_string();

    let (_server, port) = start_mock_server_multi(vec![count_response, search_response]);
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args([
            "--format",
            "json",
            "issue",
            "search",
            "project: PROJ",
            "--limit",
            "10",
        ])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 2);
}

#[test]
fn test_project_list_json() {
    let mock_response = serde_json::json!([
        {
            "id": "0-1",
            "name": "Test Project",
            "shortName": "PROJ",
            "description": "A test project"
        }
    ]);

    let (_server, port) = start_mock_server(mock_response.to_string());
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args(["-o", "json", "project", "list"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();
    assert!(json.is_array());
    // Output uses snake_case from tracker_core models
    assert_eq!(json[0]["short_name"], "PROJ");
}

#[test]
fn test_missing_url_configuration() {
    cargo_bin_cmd!("track")
        .args(["issue", "get", "PROJ-1"])
        .env("TRACKER_TOKEN", "test-token")
        .env_remove("TRACKER_URL")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("URL not configured"));
}

#[test]
fn test_missing_token_configuration() {
    cargo_bin_cmd!("track")
        .args(["issue", "get", "PROJ-1"])
        .env("TRACKER_URL", "https://test.youtrack.cloud")
        .env_remove("TRACKER_TOKEN")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("token not configured"));
}

#[test]
fn test_issue_create_command_format() {
    cargo_bin_cmd!("track")
        .args(["issue", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--project"))
        .stdout(predicate::str::contains("--summary"))
        .stdout(predicate::str::contains("--description"));
}

#[test]
fn test_issue_update_command_format() {
    cargo_bin_cmd!("track")
        .args(["issue", "update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--summary"))
        .stdout(predicate::str::contains("--description"));
}

#[test]
fn test_issue_search_with_limit_and_skip() {
    cargo_bin_cmd!("track")
        .args(["issue", "search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--limit"))
        .stdout(predicate::str::contains("--skip"));
}

#[test]
fn test_output_format_option() {
    cargo_bin_cmd!("track")
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("text"))
        .stdout(predicate::str::contains("json"));
}

#[test]
fn test_global_options_available() {
    cargo_bin_cmd!("track")
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--url"))
        .stdout(predicate::str::contains("--token"))
        .stdout(predicate::str::contains("TRACKER_URL"))
        .stdout(predicate::str::contains("TRACKER_TOKEN"));
}

#[test]
fn test_issue_delete_command_exists() {
    cargo_bin_cmd!("track")
        .args(["issue", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("delete"))
        .stdout(predicate::str::contains("Delete issue(s) by ID"));
}

#[test]
fn test_project_get_command_exists() {
    cargo_bin_cmd!("track")
        .args(["project", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("Get project by ID"));
}

#[test]
fn test_invalid_subcommand() {
    cargo_bin_cmd!("track")
        .args(["invalid-command"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn test_issue_get_requires_id() {
    cargo_bin_cmd!("track")
        .args(["issue", "get"])
        .env("TRACKER_TOKEN", "test")
        .env("TRACKER_URL", "https://test.example.com")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_issue_create_requires_project() {
    cargo_bin_cmd!("track")
        .args(["issue", "create", "--summary", "Test"])
        .env("TRACKER_TOKEN", "test")
        .env("TRACKER_URL", "https://test.example.com")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--project"));
}

#[test]
fn test_issue_create_requires_summary() {
    cargo_bin_cmd!("track")
        .args(["issue", "create", "--project", "PROJ"])
        .env("TRACKER_TOKEN", "test")
        .env("TRACKER_URL", "https://test.example.com")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--summary"));
}

// Article command integration tests

#[test]
fn test_article_commands_exist() {
    cargo_bin_cmd!("track")
        .args(["article", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("delete"));
}

#[test]
fn test_article_get_requires_id() {
    cargo_bin_cmd!("track")
        .args(["article", "get"])
        .env("TRACKER_TOKEN", "test")
        .env("TRACKER_URL", "https://test.example.com")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_article_create_requires_project() {
    cargo_bin_cmd!("track")
        .args(["article", "create", "--summary", "Test Article"])
        .env("TRACKER_TOKEN", "test")
        .env("TRACKER_URL", "https://test.example.com")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--project"));
}

#[test]
fn test_article_create_requires_summary() {
    cargo_bin_cmd!("track")
        .args(["article", "create", "--project", "KB"])
        .env("TRACKER_TOKEN", "test")
        .env("TRACKER_URL", "https://test.example.com")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--summary"));
}

#[test]
fn test_article_tree_command_exists() {
    cargo_bin_cmd!("track")
        .args(["article", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tree"))
        .stdout(predicate::str::contains("article hierarchy"));
}

#[test]
fn test_article_move_command_exists() {
    cargo_bin_cmd!("track")
        .args(["article", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("move"))
        .stdout(predicate::str::contains("Move article"));
}

#[test]
fn test_article_attachments_command_exists() {
    cargo_bin_cmd!("track")
        .args(["article", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("attachments"))
        .stdout(predicate::str::contains("List attachments"));
}

#[test]
fn test_article_comment_command_exists() {
    cargo_bin_cmd!("track")
        .args(["article", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("comment"))
        .stdout(predicate::str::contains("comments"));
}

#[test]
fn test_article_get_with_json_output() {
    let mock_response = serde_json::json!({
        "id": "123-456",
        "idReadable": "KB-A-1",
        "summary": "Test Article",
        "content": "Article content here",
        "project": {
            "id": "0-1",
            "name": "Knowledge Base",
            "shortName": "KB"
        },
        "hasChildren": false,
        "tags": [],
        "created": 1640000000000i64,
        "updated": 1640000000000i64
    });

    let (_server, port) = start_mock_server(mock_response.to_string());
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args(["--format", "json", "article", "get", "KB-A-1"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();
    assert_eq!(json["id_readable"], "KB-A-1");
    assert_eq!(json["summary"], "Test Article");
}

#[test]
fn test_article_get_with_text_output() {
    let mock_response = serde_json::json!({
        "id": "123-456",
        "idReadable": "KB-A-2",
        "summary": "Another Test Article",
        "project": {
            "id": "0-1",
            "shortName": "KB"
        },
        "hasChildren": false,
        "tags": [],
        "created": 1640000000000i64,
        "updated": 1640000000000i64
    });

    let (_server, port) = start_mock_server(mock_response.to_string());
    thread::sleep(Duration::from_millis(50));

    cargo_bin_cmd!("track")
        .args(["article", "get", "KB-A-2"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(predicate::str::contains("KB-A-2"))
        .stdout(predicate::str::contains("Another Test Article"));
}

#[test]
fn test_article_list_with_results() {
    let mock_response = serde_json::json!([
        {
            "id": "123-1",
            "idReadable": "KB-A-1",
            "summary": "First Article",
            "project": {"id": "0-1", "shortName": "KB"},
            "hasChildren": false,
            "tags": [],
            "created": 1640000000000i64,
            "updated": 1640000000000i64
        },
        {
            "id": "123-2",
            "idReadable": "KB-A-2",
            "summary": "Second Article",
            "project": {"id": "0-1", "shortName": "KB"},
            "hasChildren": true,
            "tags": [],
            "created": 1640000000000i64,
            "updated": 1640000000000i64
        }
    ]);

    let (_server, port) = start_mock_server(mock_response.to_string());
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args(["--format", "json", "article", "list", "--limit", "10"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 2);
}

#[test]
fn test_article_search_with_query() {
    cargo_bin_cmd!("track")
        .args(["article", "search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("query"))
        .stdout(predicate::str::contains("--limit"))
        .stdout(predicate::str::contains("--skip"));
}

#[test]
fn test_article_create_command_format() {
    cargo_bin_cmd!("track")
        .args(["article", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--project"))
        .stdout(predicate::str::contains("--summary"))
        .stdout(predicate::str::contains("--content"))
        .stdout(predicate::str::contains("--content-file"))
        .stdout(predicate::str::contains("--parent"))
        .stdout(predicate::str::contains("--tag"));
}

#[test]
fn test_article_update_command_format() {
    cargo_bin_cmd!("track")
        .args(["article", "update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--summary"))
        .stdout(predicate::str::contains("--content"))
        .stdout(predicate::str::contains("--content-file"));
}

// ============================================================================
// Shell Completions Tests
// ============================================================================

#[test]
fn test_completions_command_exists() {
    cargo_bin_cmd!("track")
        .args(["completions", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Generate shell completions"))
        .stdout(predicate::str::contains("bash"))
        .stdout(predicate::str::contains("zsh"))
        .stdout(predicate::str::contains("fish"));
}

#[test]
fn test_completions_bash_output() {
    cargo_bin_cmd!("track")
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_track"))
        .stdout(predicate::str::contains("COMPREPLY"));
}

#[test]
fn test_completions_zsh_output() {
    cargo_bin_cmd!("track")
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef track"))
        .stdout(predicate::str::contains("_track"));
}

#[test]
fn test_completions_fish_output() {
    cargo_bin_cmd!("track")
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -c track"));
}

#[test]
fn test_completions_no_config_required() {
    // Completions should work without any configuration
    cargo_bin_cmd!("track")
        .args(["completions", "bash"])
        .env_remove("TRACKER_URL")
        .env_remove("TRACKER_TOKEN")
        .assert()
        .success();
}

// ============================================================================
// Pagination Hint Tests
// ============================================================================

#[test]
fn test_pagination_hint_on_full_page() {
    // YouTrack search: first request = count, second request = search results
    let count_response = serde_json::json!({ "count": 10 }).to_string();
    let search_response = serde_json::json!([
        {
            "id": "2-1", "idReadable": "PROJ-1", "summary": "Issue 1",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [], "tags": [],
            "created": 1640000000000i64, "updated": 1640000000000i64
        },
        {
            "id": "2-2", "idReadable": "PROJ-2", "summary": "Issue 2",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [], "tags": [],
            "created": 1640000000000i64, "updated": 1640000000000i64
        }
    ])
    .to_string();

    let (_server, port) = start_mock_server_multi(vec![count_response, search_response]);
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args(["issue", "search", "project: PROJ", "--limit", "2"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("results shown"),
        "Should show pagination hint on full page, got stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("--all"),
        "Should suggest --all flag, got stderr: {}",
        stderr
    );
}

#[test]
fn test_no_pagination_hint_on_partial_page() {
    // Return fewer items than limit — no hint expected
    let count_response = serde_json::json!({ "count": 2 }).to_string();
    let search_response = serde_json::json!([
        {
            "id": "2-1", "idReadable": "PROJ-1", "summary": "Issue 1",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [], "tags": [],
            "created": 1640000000000i64, "updated": 1640000000000i64
        },
        {
            "id": "2-2", "idReadable": "PROJ-2", "summary": "Issue 2",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [], "tags": [],
            "created": 1640000000000i64, "updated": 1640000000000i64
        }
    ])
    .to_string();

    let (_server, port) = start_mock_server_multi(vec![count_response, search_response]);
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args(["issue", "search", "project: PROJ", "--limit", "10"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("results shown"),
        "Should NOT show pagination hint on partial page, got stderr: {}",
        stderr
    );
}

#[test]
fn test_no_pagination_hint_in_json_mode() {
    let count_response = serde_json::json!({ "count": 10 }).to_string();
    let search_response = serde_json::json!([
        {
            "id": "2-1", "idReadable": "PROJ-1", "summary": "Issue 1",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [], "tags": [],
            "created": 1640000000000i64, "updated": 1640000000000i64
        },
        {
            "id": "2-2", "idReadable": "PROJ-2", "summary": "Issue 2",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [], "tags": [],
            "created": 1640000000000i64, "updated": 1640000000000i64
        }
    ])
    .to_string();

    let (_server, port) = start_mock_server_multi(vec![count_response, search_response]);
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args([
            "--format",
            "json",
            "issue",
            "search",
            "project: PROJ",
            "--limit",
            "2",
        ])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("results shown"),
        "Should NOT show pagination hint in JSON mode, got stderr: {}",
        stderr
    );
}

#[test]
fn test_pagination_hint_shows_total() {
    let count_response = serde_json::json!({ "count": 10 }).to_string();
    let search_response = serde_json::json!([
        {
            "id": "2-1", "idReadable": "PROJ-1", "summary": "Issue 1",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [], "tags": [],
            "created": 1640000000000i64, "updated": 1640000000000i64
        },
        {
            "id": "2-2", "idReadable": "PROJ-2", "summary": "Issue 2",
            "project": {"id": "0-1", "shortName": "PROJ"},
            "customFields": [], "tags": [],
            "created": 1640000000000i64, "updated": 1640000000000i64
        }
    ])
    .to_string();

    let (_server, port) = start_mock_server_multi(vec![count_response, search_response]);
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args(["issue", "search", "project: PROJ", "--limit", "2"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("of 10 total"),
        "Should show total count in hint, got stderr: {}",
        stderr
    );
}

// ============================================================================
// Live Integration Tests (YouTrack)
//
// These tests run against a real YouTrack instance and are ignored by default.
// To run them:
//   cargo test --package track --test youtrack_integration_tests -- --ignored
//
// Prerequisites:
//   - Ensure .track.toml in the project root contains a [youtrack] section with:
//     [youtrack]
//     url = "https://your-instance.youtrack.cloud"
//     token = "perm-your-token"
//
//   - Have at least one project (e.g., "track-cli")
// ============================================================================

/// Get the path to the .track.toml config file at project root
fn config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".track.toml")
}

/// Check if the config file exists (skip tests if not)
fn config_exists() -> bool {
    config_path().exists()
}

/// The default project shortName for YouTrack tests
const YOUTRACK_PROJECT: &str = "OGIT";

/// Helper to build a track command with YouTrack config (default backend)
fn track_yt() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["--config"])
        .arg(config_path())
        .timeout(Duration::from_secs(30));
    cmd
}

/// Helper to build a track command with YouTrack config + JSON output
fn track_yt_json() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-o", "json", "--config"])
        .arg(config_path())
        .timeout(Duration::from_secs(30));
    cmd
}

// ============================================================================
// Connection & Configuration
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_config_test_command() {
    if !config_exists() {
        eprintln!("Skipping: .track.toml not found");
        return;
    }

    track_yt()
        .args(["config", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Connected to"));
}

// ============================================================================
// Project Operations
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_project_list() {
    if !config_exists() {
        return;
    }

    let output = track_yt_json()
        .args(["project", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json.is_array(), "Project list should be an array");
    let projects = json.as_array().unwrap();
    assert!(!projects.is_empty(), "Should have at least one project");

    // Verify project structure
    let project = &projects[0];
    assert!(project["id"].is_string(), "Project should have id");
    assert!(project["name"].is_string(), "Project should have name");
    assert!(
        project["short_name"].is_string(),
        "Project should have short_name"
    );
}

#[test]
#[ignore]
fn test_youtrack_project_list_text_output() {
    if !config_exists() {
        return;
    }

    track_yt()
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
#[ignore]
fn test_youtrack_project_get() {
    if !config_exists() {
        return;
    }

    let output = track_yt_json()
        .args(["project", "get", YOUTRACK_PROJECT])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json["name"].is_string());
    assert!(json["short_name"].is_string());
}

#[test]
#[ignore]
fn test_youtrack_project_custom_fields() {
    if !config_exists() {
        return;
    }

    let output = track_yt_json()
        .args(["project", "fields", YOUTRACK_PROJECT])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json.is_array(), "Custom fields should be an array");
}

// ============================================================================
// Issue CRUD Operations
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_issue_create_and_delete() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Integration Test Issue - DELETE ME",
            "-d",
            "This is an automated test issue created by integration tests.",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();

    let issue_id = created["id_readable"].as_str().unwrap();
    assert!(
        !issue_id.is_empty(),
        "Created issue should have id_readable"
    );

    // Get the issue we just created
    let get_output = track_yt_json()
        .args(["issue", "get", issue_id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    assert_eq!(fetched["summary"], "Integration Test Issue - DELETE ME");

    // Delete the issue
    track_yt()
        .args(["issue", "delete", issue_id])
        .assert()
        .success();

    // Verify deletion - should fail to get
    track_yt()
        .args(["issue", "get", issue_id])
        .assert()
        .failure();
}

#[test]
#[ignore]
fn test_youtrack_issue_update() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Test Issue for Update",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_id = created["id_readable"].as_str().unwrap().to_string();

    // Update the issue
    track_yt()
        .args([
            "issue",
            "update",
            &issue_id,
            "--summary",
            "Updated Test Issue Summary",
            "--description",
            "Updated description via integration test",
        ])
        .assert()
        .success();

    // Verify update
    let get_output = track_yt_json()
        .args(["issue", "get", &issue_id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    assert_eq!(fetched["summary"], "Updated Test Issue Summary");

    // Clean up
    track_yt()
        .args(["issue", "delete", &issue_id])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_youtrack_issue_search() {
    if !config_exists() {
        return;
    }

    let output = track_yt_json()
        .args([
            "issue",
            "search",
            &format!("project: {YOUTRACK_PROJECT} #Unresolved"),
            "--limit",
            "5",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json.is_array(), "Search results should be an array");
}

#[test]
#[ignore]
fn test_youtrack_issue_search_pagination() {
    if !config_exists() {
        return;
    }

    let output = track_yt_json()
        .args([
            "issue",
            "search",
            &format!("project: {YOUTRACK_PROJECT}"),
            "--limit",
            "1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json.is_array());
    let results = json.as_array().unwrap();
    assert!(
        results.len() <= 1,
        "Should return at most 1 result with --limit 1, got {}",
        results.len()
    );
}

#[test]
#[ignore]
fn test_youtrack_issue_get_shortcut() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Test Shortcut Access",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_id = created["id_readable"].as_str().unwrap().to_string();

    // Use the shortcut: `track <ID>` instead of `track issue get <ID>`
    track_yt()
        .arg(&issue_id)
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Shortcut Access"));

    // Clean up
    track_yt()
        .args(["issue", "delete", &issue_id])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_youtrack_issue_get_with_full_flag() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Test Full Flag",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_id = created["id_readable"].as_str().unwrap().to_string();

    // Get with --full flag
    let full_output = track_yt_json()
        .args(["issue", "get", &issue_id, "--full"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let full_str = String::from_utf8(full_output).unwrap();
    let full: Value = serde_json::from_str(&full_str).unwrap();

    // --full should include extra fields
    assert!(
        full.get("links").is_some() || full.get("comments").is_some(),
        "Full output should include links or comments"
    );

    // Clean up
    track_yt()
        .args(["issue", "delete", &issue_id])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_youtrack_issue_get_not_found() {
    if !config_exists() {
        return;
    }

    track_yt()
        .args(["issue", "get", "NONEXIST-99999"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Not found")
                .or(predicate::str::contains("not found"))
                .or(predicate::str::contains("404")),
        );
}

#[test]
#[ignore]
fn test_youtrack_invalid_project() {
    if !config_exists() {
        return;
    }

    track_yt()
        .args([
            "issue",
            "create",
            "-p",
            "NONEXISTENT-PROJECT-XYZ",
            "-s",
            "Should fail",
        ])
        .assert()
        .failure();
}

// ============================================================================
// Issue Linking
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_issue_link() {
    if !config_exists() {
        return;
    }

    // Create two issues
    let create1_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Link Test Issue 1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create2_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Link Test Issue 2",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue1: Value = serde_json::from_str(&String::from_utf8(create1_output).unwrap()).unwrap();
    let issue2: Value = serde_json::from_str(&String::from_utf8(create2_output).unwrap()).unwrap();
    let id1 = issue1["id_readable"].as_str().unwrap().to_string();
    let id2 = issue2["id_readable"].as_str().unwrap().to_string();

    // Link the issues
    track_yt()
        .args(["issue", "link", &id1, &id2, "-t", "relates"])
        .assert()
        .success();

    // Verify link exists via --full get
    let get_output = track_yt_json()
        .args(["issue", "get", &id1, "--full"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    if let Some(links) = fetched["links"].as_array() {
        assert!(!links.is_empty(), "Issue should have at least one link");
    }

    // Clean up
    for id in [&id1, &id2] {
        track_yt().args(["issue", "delete", id]).assert().success();
    }
}

#[test]
#[ignore]
fn test_youtrack_issue_link_subtask() {
    if !config_exists() {
        return;
    }

    // Create parent and child
    let parent_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Subtask Parent Issue",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let child_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Subtask Child Issue",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parent: Value = serde_json::from_str(&String::from_utf8(parent_output).unwrap()).unwrap();
    let child: Value = serde_json::from_str(&String::from_utf8(child_output).unwrap()).unwrap();
    let parent_id = parent["id_readable"].as_str().unwrap().to_string();
    let child_id = child["id_readable"].as_str().unwrap().to_string();

    // Link as subtask (child is subtask of parent)
    track_yt()
        .args(["issue", "link", &child_id, &parent_id, "-t", "subtask"])
        .assert()
        .success();

    // Clean up
    for id in [&child_id, &parent_id] {
        track_yt().args(["issue", "delete", id]).assert().success();
    }
}

#[test]
#[ignore]
fn test_youtrack_issue_link_parent() {
    if !config_exists() {
        return;
    }

    // Create parent and child
    let parent_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Parent Link Parent Issue",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let child_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Parent Link Child Issue",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parent: Value = serde_json::from_str(&String::from_utf8(parent_output).unwrap()).unwrap();
    let child: Value = serde_json::from_str(&String::from_utf8(child_output).unwrap()).unwrap();
    let parent_id = parent["id_readable"].as_str().unwrap().to_string();
    let child_id = child["id_readable"].as_str().unwrap().to_string();

    // Link as parent (parent_id is parent of child_id)
    track_yt()
        .args(["issue", "link", &parent_id, &child_id, "-t", "parent"])
        .assert()
        .success();

    // Clean up
    for id in [&child_id, &parent_id] {
        track_yt().args(["issue", "delete", id]).assert().success();
    }
}

// ============================================================================
// Comment Operations
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_issue_comments() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Test Issue for Comments",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_id = created["id_readable"].as_str().unwrap().to_string();

    // Add a comment
    track_yt()
        .args([
            "issue",
            "comment",
            &issue_id,
            "-m",
            "This is a test comment from integration tests",
        ])
        .assert()
        .success();

    // Get comments
    let comments_output = track_yt_json()
        .args(["issue", "comments", &issue_id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let comments_str = String::from_utf8(comments_output).unwrap();
    let comments: Value = serde_json::from_str(&comments_str).unwrap();
    assert!(comments.is_array());
    let comments_arr = comments.as_array().unwrap();
    assert!(!comments_arr.is_empty(), "Should have at least one comment");

    // Verify comment text
    let found = comments_arr.iter().any(|c| {
        c["text"]
            .as_str()
            .is_some_and(|t| t.contains("test comment"))
    });
    assert!(found, "Should find the comment we added");

    // Clean up
    track_yt()
        .args(["issue", "delete", &issue_id])
        .assert()
        .success();
}

// ============================================================================
// Tag Operations
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_tags_list() {
    if !config_exists() {
        return;
    }

    let output = track_yt_json()
        .args(["tags", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json.is_array(), "Tags list should be an array");
}

#[test]
#[ignore]
fn test_youtrack_tags_list_text_output() {
    if !config_exists() {
        return;
    }

    track_yt().args(["tags", "list"]).assert().success();
}

#[test]
#[ignore]
fn test_youtrack_tags_create_and_delete() {
    if !config_exists() {
        return;
    }

    let tag_name = format!(
        "test-tag-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            % 100000
    );

    // Create a tag
    let output = track_yt_json()
        .args(["tags", "create", &tag_name])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let created: Value = serde_json::from_str(&String::from_utf8(output).unwrap()).unwrap();
    assert_eq!(created["name"].as_str().unwrap(), tag_name);

    // Verify it shows in list
    let list_output = track_yt_json()
        .args(["tags", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let tags: Vec<Value> = serde_json::from_str(&String::from_utf8(list_output).unwrap()).unwrap();
    assert!(
        tags.iter().any(|t| t["name"].as_str() == Some(&*tag_name)),
        "Created tag should appear in list"
    );

    // Delete the tag
    track_yt()
        .args(["tags", "delete", &tag_name])
        .assert()
        .success();

    // Verify it's gone
    let list_output2 = track_yt_json()
        .args(["tags", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let tags2: Vec<Value> =
        serde_json::from_str(&String::from_utf8(list_output2).unwrap()).unwrap();
    assert!(
        !tags2.iter().any(|t| t["name"].as_str() == Some(&*tag_name)),
        "Deleted tag should not appear in list"
    );
}

#[test]
#[ignore]
fn test_youtrack_tags_update() {
    if !config_exists() {
        return;
    }

    let tag_name = format!(
        "test-upd-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            % 100000
    );

    let new_name = format!("{tag_name}-renamed");

    // Create a tag
    track_yt_json()
        .args(["tags", "create", &tag_name])
        .assert()
        .success();

    // Update its name
    track_yt_json()
        .args(["tags", "update", &tag_name, "--new-name", &new_name])
        .assert()
        .success();

    // Verify the renamed tag exists
    let list_output = track_yt_json()
        .args(["tags", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let tags: Vec<Value> = serde_json::from_str(&String::from_utf8(list_output).unwrap()).unwrap();
    assert!(
        tags.iter().any(|t| t["name"].as_str() == Some(&*new_name)),
        "Renamed tag should appear in list"
    );

    // Clean up
    track_yt()
        .args(["tags", "delete", &new_name])
        .assert()
        .success();
}

// ============================================================================
// Articles / Knowledge Base (YouTrack-specific)
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_article_create_and_delete() {
    if !config_exists() {
        return;
    }

    // Create an article
    let create_output = track_yt_json()
        .args([
            "article",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Integration Test Article - DELETE ME",
            "--content",
            "This is test article content.",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let article_id = created["id_readable"].as_str().unwrap().to_string();
    assert!(!article_id.is_empty(), "Article should have id_readable");

    // Get the article
    let get_output = track_yt_json()
        .args(["article", "get", &article_id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    assert_eq!(fetched["summary"], "Integration Test Article - DELETE ME");

    // Delete the article
    track_yt()
        .args(["article", "delete", &article_id])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_youtrack_article_search() {
    if !config_exists() {
        return;
    }

    let output = track_yt_json()
        .args([
            "article",
            "search",
            &format!("project: {YOUTRACK_PROJECT}"),
            "--limit",
            "5",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json.is_array(), "Article search results should be an array");
}

#[test]
#[ignore]
fn test_youtrack_article_comments() {
    if !config_exists() {
        return;
    }

    // Create an article
    let create_output = track_yt_json()
        .args([
            "article",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Article Comment Test",
            "--content",
            "Content for comment test.",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let article_id = created["id_readable"].as_str().unwrap().to_string();

    // Add a comment
    track_yt()
        .args([
            "article",
            "comment",
            &article_id,
            "-m",
            "Test article comment from integration tests",
        ])
        .assert()
        .success();

    // Get comments
    let comments_output = track_yt_json()
        .args(["article", "comments", &article_id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let comments_str = String::from_utf8(comments_output).unwrap();
    let comments: Value = serde_json::from_str(&comments_str).unwrap();
    assert!(comments.is_array());
    let comments_arr = comments.as_array().unwrap();
    assert!(
        !comments_arr.is_empty(),
        "Should have at least one article comment"
    );

    // Clean up
    track_yt()
        .args(["article", "delete", &article_id])
        .assert()
        .success();
}

// ============================================================================
// CLI Aliases
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_cli_aliases() {
    if !config_exists() {
        return;
    }

    // `p ls` should work as alias for `project list`
    track_yt()
        .args(["p", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());

    // `i s` should work as alias for `issue search`
    track_yt()
        .args([
            "i",
            "s",
            &format!("project: {YOUTRACK_PROJECT}"),
            "--limit",
            "1",
        ])
        .assert()
        .success();
}

// ============================================================================
// Output Format
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_json_output_parseable() {
    if !config_exists() {
        return;
    }

    let output = track_yt_json()
        .args(["project", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).expect("Output should be valid JSON");
    assert!(json.is_array());
}

#[test]
#[ignore]
fn test_youtrack_text_output_readable() {
    if !config_exists() {
        return;
    }

    // Text output for project list should contain parentheses (short_name)
    track_yt()
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("("));
}

// ============================================================================
// Feature Parity / Structure
// ============================================================================

#[test]
#[ignore]
fn test_youtrack_issue_structure() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Structure Test Issue",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let json: Value = serde_json::from_str(&create_str).unwrap();

    // Verify core issue structure
    assert!(json.get("id").is_some(), "Issue should have 'id'");
    assert!(
        json.get("id_readable").is_some(),
        "Issue should have 'id_readable'"
    );
    assert!(json.get("summary").is_some(), "Issue should have 'summary'");
    assert!(json.get("project").is_some(), "Issue should have 'project'");
    assert!(json.get("created").is_some(), "Issue should have 'created'");
    assert!(json.get("updated").is_some(), "Issue should have 'updated'");

    // Clean up
    let issue_id = json["id_readable"].as_str().unwrap();
    track_yt()
        .args(["issue", "delete", issue_id])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_youtrack_project_structure() {
    if !config_exists() {
        return;
    }

    let output = track_yt_json()
        .args(["project", "get", YOUTRACK_PROJECT])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json.get("id").is_some(), "Project should have 'id'");
    assert!(json.get("name").is_some(), "Project should have 'name'");
    assert!(
        json.get("short_name").is_some(),
        "Project should have 'short_name'"
    );
}

#[test]
#[ignore]
fn test_youtrack_comment_structure() {
    if !config_exists() {
        return;
    }

    // Create an issue and add a comment
    let create_output = track_yt_json()
        .args([
            "issue",
            "create",
            "-p",
            YOUTRACK_PROJECT,
            "-s",
            "Comment Structure Test",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let created: Value = serde_json::from_str(&String::from_utf8(create_output).unwrap()).unwrap();
    let issue_id = created["id_readable"].as_str().unwrap().to_string();

    // Add a comment
    track_yt()
        .args([
            "issue",
            "comment",
            &issue_id,
            "-m",
            "Structure test comment",
        ])
        .assert()
        .success();

    // Get comments and verify structure
    let comments_output = track_yt_json()
        .args(["issue", "comments", &issue_id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let comments: Value =
        serde_json::from_str(&String::from_utf8(comments_output).unwrap()).unwrap();
    let comments_arr = comments.as_array().unwrap();
    assert!(!comments_arr.is_empty());

    let comment = &comments_arr[0];
    assert!(comment.get("id").is_some(), "Comment should have 'id'");
    assert!(comment.get("text").is_some(), "Comment should have 'text'");
    assert!(
        comment.get("author").is_some(),
        "Comment should have 'author'"
    );
    assert!(
        comment.get("created").is_some(),
        "Comment should have 'created'"
    );

    // Clean up
    track_yt()
        .args(["issue", "delete", &issue_id])
        .assert()
        .success();
}

// ============================================================================
// Unlink Tests
// ============================================================================

#[test]
fn test_unlink_text_output() {
    // The DELETE endpoint returns 200 with empty body
    let mock_response = String::new();
    let (_server, port) = start_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    cargo_bin_cmd!("track")
        .args(["issue", "unlink", "PROJ-123", "142-3t/PROJ-456"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(predicate::str::contains("PROJ-123"))
        .stdout(predicate::str::contains("unlinked"))
        .stdout(predicate::str::contains("142-3t/PROJ-456"));
}

#[test]
fn test_unlink_json_output() {
    let mock_response = String::new();
    let (_server, port) = start_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args([
            "--format",
            "json",
            "issue",
            "unlink",
            "PROJ-123",
            "142-3t/PROJ-456",
        ])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["source"], "PROJ-123");
    assert_eq!(json["linkId"], "142-3t/PROJ-456");
}

#[test]
fn test_unlink_invalid_link_id_format() {
    // A link ID without "/" is invalid for YouTrack — the trait_impl rejects it
    // before making any HTTP call, so no mock server is needed.
    cargo_bin_cmd!("track")
        .args(["issue", "unlink", "PROJ-123", "bad-id-no-slash"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", "http://127.0.0.1:1")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected format"));
}

#[test]
fn test_unlink_alias_ul() {
    let mock_response = String::new();
    let (_server, port) = start_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    cargo_bin_cmd!("track")
        .args(["issue", "ul", "PROJ-123", "142-3t/PROJ-456"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(predicate::str::contains("unlinked"));
}

// ============================================================================
// Link Type Mapping Tests (mock-based)
// ============================================================================

/// Mock response for GET /api/issues/{id}/links — returns link buckets
/// that the client uses to find the correct bucket ID for a link type.
fn mock_link_buckets_response() -> String {
    serde_json::json!([
        {
            "id": "100-0b",
            "direction": "BOTH",
            "linkType": {
                "id": "100-0",
                "name": "Relates",
                "sourceToTarget": "relates to",
                "targetToSource": "is related to",
                "directed": false
            },
            "issues": []
        },
        {
            "id": "101-0o",
            "direction": "OUTWARD",
            "linkType": {
                "id": "101-0",
                "name": "Depend",
                "sourceToTarget": "depends on",
                "targetToSource": "is required for",
                "directed": true
            },
            "issues": []
        },
        {
            "id": "101-0i",
            "direction": "INWARD",
            "linkType": {
                "id": "101-0",
                "name": "Depend",
                "sourceToTarget": "depends on",
                "targetToSource": "is required for",
                "directed": true
            },
            "issues": []
        },
        {
            "id": "102-0o",
            "direction": "OUTWARD",
            "linkType": {
                "id": "102-0",
                "name": "Duplicate",
                "sourceToTarget": "duplicates",
                "targetToSource": "is duplicated by",
                "directed": true
            },
            "issues": []
        }
    ])
    .to_string()
}

#[test]
fn test_youtrack_link_relates_text_output() {
    // YouTrack link requires 2 calls: GET links (find bucket ID), POST add issue to link
    let responses = vec![
        mock_link_buckets_response(), // GET /api/issues/PROJ-1/links
        String::new(),                // POST /api/issues/PROJ-1/links/100-0b/issues
    ];
    let (_server, port) = start_mock_server_multi(responses);
    thread::sleep(Duration::from_millis(50));

    cargo_bin_cmd!("track")
        .args(["issue", "link", "PROJ-1", "PROJ-2", "-t", "relates"])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(predicate::str::contains("PROJ-1"))
        .stdout(predicate::str::contains("relates to"))
        .stdout(predicate::str::contains("PROJ-2"));
}

#[test]
fn test_youtrack_link_depends_json_output() {
    let responses = vec![
        mock_link_buckets_response(), // GET /api/issues/PROJ-1/links
        String::new(),                // POST /api/issues/PROJ-1/links/101-0o/issues
    ];
    let (_server, port) = start_mock_server_multi(responses);
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args([
            "-o", "json", "issue", "link", "PROJ-1", "PROJ-2", "-t", "depends",
        ])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_str(&String::from_utf8(output).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["source"], "PROJ-1");
    assert_eq!(json["target"], "PROJ-2");
    assert_eq!(json["linkType"], "depends");
    assert_eq!(json["description"], "depends on");
}

#[test]
fn test_youtrack_link_required_uses_inward_direction() {
    // "required" should look for the INWARD Depend bucket
    let responses = vec![
        mock_link_buckets_response(), // GET /api/issues/PROJ-1/links
        String::new(),                // POST /api/issues/PROJ-1/links/101-0i/issues
    ];
    let (_server, port) = start_mock_server_multi(responses);
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args([
            "-o", "json", "issue", "link", "PROJ-1", "PROJ-2", "-t", "required",
        ])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_str(&String::from_utf8(output).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["linkType"], "required");
    assert_eq!(json["description"], "is required for");
}

#[test]
fn test_youtrack_link_custom_type_passthrough() {
    // Custom type "clones" gets passed through with BOTH direction.
    // It resolves to "clones" (unknown → default "Relates" via resolve_link_type).
    // The mock returns Relates bucket for BOTH direction.
    let responses = vec![
        mock_link_buckets_response(), // GET links — "Relates" has BOTH direction
        String::new(),                // POST add issue
    ];
    let (_server, port) = start_mock_server_multi(responses);
    thread::sleep(Duration::from_millis(50));

    let output = cargo_bin_cmd!("track")
        .args([
            "-o", "json", "issue", "link", "PROJ-1", "PROJ-2", "-t", "clones",
        ])
        .env("TRACKER_TOKEN", "test-token")
        .env("TRACKER_URL", format!("http://127.0.0.1:{}", port))
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_str(&String::from_utf8(output).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["linkType"], "clones");
}
