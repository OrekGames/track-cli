use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
use std::net::TcpListener;
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
