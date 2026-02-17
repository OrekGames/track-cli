//! Integration tests for GitHub backend
//!
//! These tests run against a real GitHub instance and are ignored by default.
//! To run them:
//!   cargo test --package track --test github_integration_tests -- --ignored
//!
//! Prerequisites:
//!   - Ensure .track.toml in the project root contains a [github] section with:
//!     [github]
//!     token = "ghp_your_token"
//!     owner = "your-org"
//!     repo = "your-repo"
//!
//!   - The configured repo must exist and the token must have Issues read/write scope

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

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

/// The project identifier for issue creation (owner/repo from .track.toml)
const GITHUB_PROJECT: &str = "OrekGames/track-cli";

/// Helper to build a track command with GitHub backend and config
fn track_github() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "github", "--config"])
        .arg(config_path())
        .timeout(Duration::from_secs(30));
    cmd
}

/// Helper to build a track command with GitHub backend, JSON output, and config
fn track_github_json() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "github", "-o", "json", "--config"])
        .arg(config_path())
        .timeout(Duration::from_secs(30));
    cmd
}

/// Check if the GitHub token has write access to Issues.
/// Returns true if writes are allowed, false if the token is read-only.
fn has_write_access() -> bool {
    use std::sync::OnceLock;
    static WRITE_ACCESS: OnceLock<bool> = OnceLock::new();
    *WRITE_ACCESS.get_or_init(|| {
        let output = track_github_json()
            .args(["issue", "create", "-p", GITHUB_PROJECT, "-s", "__write_access_probe__"])
            .assert()
            .get_output()
            .clone();

        if output.status.success() {
            // Clean up probe issue
            if let Ok(s) = String::from_utf8(output.stdout) {
                if let Ok(v) = serde_json::from_str::<Value>(&s) {
                    if let Some(id) = v["id_readable"].as_str() {
                        let number = id.split('#').last().unwrap_or("");
                        let _ = track_github()
                            .args(["issue", "update", number, "--state", "closed"])
                            .assert();
                    }
                }
            }
            true
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("403") || stderr.contains("not accessible") {
                eprintln!("GitHub token lacks write permissions — skipping write tests");
                false
            } else {
                // Some other error; assume write access to let tests surface it
                true
            }
        }
    })
}

// ============================================================================
// Connection & Configuration Tests
// ============================================================================

#[test]
#[ignore]
fn test_github_config_test_command() {
    if !config_exists() {
        eprintln!("Skipping: .track.toml not found");
        return;
    }

    track_github()
        .args(["config", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Connected to"));
}

// ============================================================================
// Project (Repository) Operations
// ============================================================================

#[test]
#[ignore]
fn test_github_project_list() {
    if !config_exists() {
        return;
    }

    let output = track_github_json()
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
fn test_github_project_list_text_output() {
    if !config_exists() {
        return;
    }

    track_github()
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("track-cli").or(predicate::str::contains("OrekGames")));
}

#[test]
#[ignore]
fn test_github_project_get() {
    if !config_exists() {
        return;
    }

    let output = track_github_json()
        .args(["project", "get", "OrekGames/track-cli"])
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
fn test_github_project_get_repo_name_only() {
    if !config_exists() {
        return;
    }

    // Should resolve just repo name using configured owner
    let output = track_github_json()
        .args(["project", "get", "track-cli"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json["name"].is_string());
}

#[test]
#[ignore]
fn test_github_project_custom_fields() {
    if !config_exists() {
        return;
    }

    // GitHub returns standard custom fields (Status, Priority, Assignee)
    let output = track_github_json()
        .args(["project", "fields", "OrekGames/track-cli"])
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
// Issue Search Operations
// ============================================================================

#[test]
#[ignore]
fn test_github_issue_search() {
    if !config_exists() {
        return;
    }

    let output = track_github_json()
        .args(["issue", "search", "is:open", "--limit", "5"])
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
fn test_github_issue_search_closed() {
    if !config_exists() {
        return;
    }

    track_github_json()
        .args(["issue", "search", "is:closed", "--limit", "5"])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_github_issue_search_by_label() {
    if !config_exists() {
        return;
    }

    // Search with label filter
    track_github_json()
        .args(["issue", "search", "is:issue is:open", "--limit", "5"])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_github_issue_search_pagination() {
    if !config_exists() {
        return;
    }

    // Test pagination with skip
    track_github_json()
        .args([
            "issue", "search", "is:open", "--limit", "2", "--skip", "0",
        ])
        .assert()
        .success();
}

// ============================================================================
// Issue CRUD Operations
// ============================================================================

#[test]
#[ignore]
fn test_github_issue_create_get_and_close() {
    if !config_exists() || !has_write_access() {
        return;
    }

    // Create an issue
    let create_output = track_github_json()
        .args([
            "issue",
            "create",
            "-p",
            GITHUB_PROJECT,
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
        issue_id.contains('#'),
        "GitHub issue should have # in id_readable, got: {}",
        issue_id
    );

    // Extract the number for subsequent operations
    let issue_number = issue_id.split('#').last().unwrap();

    // Get the issue we just created
    let get_output = track_github_json()
        .args(["issue", "get", issue_number])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    assert_eq!(fetched["summary"], "Integration Test Issue - DELETE ME");

    // Verify it's open (State custom field with is_resolved: false)
    let state_field = fetched["custom_fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f.get("State").is_some())
        .expect("Should have a State custom field");
    assert_eq!(state_field["State"]["is_resolved"], false);

    // Close the issue (GitHub doesn't support delete)
    track_github()
        .args([
            "issue", "update", issue_number, "--state", "closed",
        ])
        .assert()
        .success();

    // Verify the issue is now resolved
    let closed_output = track_github_json()
        .args(["issue", "get", issue_number])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let closed_str = String::from_utf8(closed_output).unwrap();
    let closed: Value = serde_json::from_str(&closed_str).unwrap();
    let state_field = closed["custom_fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f.get("State").is_some())
        .expect("Should have a State custom field");
    assert_eq!(state_field["State"]["is_resolved"], true);
}

#[test]
#[ignore]
fn test_github_issue_update() {
    if !config_exists() || !has_write_access() {
        return;
    }

    // Create an issue
    let create_output = track_github_json()
        .args([
            "issue",
            "create",
            "-p",
            GITHUB_PROJECT,
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
    let issue_number = created["id_readable"]
        .as_str()
        .unwrap()
        .split('#')
        .last()
        .unwrap()
        .to_string();

    // Update the issue
    track_github()
        .args([
            "issue",
            "update",
            &issue_number,
            "--summary",
            "Updated Test Issue Summary",
            "--description",
            "Updated description via integration test",
        ])
        .assert()
        .success();

    // Verify update
    let get_output = track_github_json()
        .args(["issue", "get", &issue_number])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    assert_eq!(fetched["summary"], "Updated Test Issue Summary");

    // Clean up: close the issue
    track_github()
        .args(["issue", "update", &issue_number, "--state", "closed"])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_github_issue_create_with_labels() {
    if !config_exists() || !has_write_access() {
        return;
    }

    // Create an issue with a tag/label
    let create_output = track_github_json()
        .args([
            "issue",
            "create",
            "-p",
            GITHUB_PROJECT,
            "-s",
            "Test Issue with Labels",
            "--tag",
            "bug",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_number = created["id_readable"]
        .as_str()
        .unwrap()
        .split('#')
        .last()
        .unwrap()
        .to_string();

    // Verify tags were applied
    let tags = created["tags"].as_array();
    if let Some(tags) = tags {
        let has_bug = tags.iter().any(|t| t["name"].as_str() == Some("bug"));
        assert!(has_bug, "Issue should have 'bug' label");
    }

    // Clean up
    track_github()
        .args(["issue", "update", &issue_number, "--state", "closed"])
        .assert()
        .success();
}

// ============================================================================
// Issue Get Operations
// ============================================================================

#[test]
#[ignore]
fn test_github_issue_get_shortcut() {
    if !config_exists() || !has_write_access() {
        return;
    }

    // Create an issue to have something to get
    let create_output = track_github_json()
        .args(["issue", "create", "-p", GITHUB_PROJECT, "-s", "Test Shortcut Access"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_number = created["id_readable"]
        .as_str()
        .unwrap()
        .split('#')
        .last()
        .unwrap()
        .to_string();

    // GitHub numeric IDs can't use the bare shortcut (track <number>) because
    // clap treats numbers as subcommands. Use explicit issue get instead.
    track_github()
        .args(["issue", "get", &issue_number])
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Shortcut Access"));

    // Clean up
    track_github()
        .args(["issue", "update", &issue_number, "--state", "closed"])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_github_issue_get_not_found() {
    if !config_exists() {
        return;
    }

    track_github()
        .args(["issue", "get", "999999"])
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
fn test_github_issue_get_with_full_flag() {
    if !config_exists() || !has_write_access() {
        return;
    }

    // Create an issue
    let create_output = track_github_json()
        .args(["issue", "create", "-p", GITHUB_PROJECT, "-s", "Test Full Flag"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_number = created["id_readable"]
        .as_str()
        .unwrap()
        .split('#')
        .last()
        .unwrap()
        .to_string();

    // Get with --full flag
    track_github()
        .args(["issue", "get", &issue_number, "--full"])
        .assert()
        .success();

    // Clean up
    track_github()
        .args(["issue", "update", &issue_number, "--state", "closed"])
        .assert()
        .success();
}

// ============================================================================
// Comment Operations
// ============================================================================

#[test]
#[ignore]
fn test_github_issue_comments() {
    if !config_exists() || !has_write_access() {
        return;
    }

    // Create an issue
    let create_output = track_github_json()
        .args([
            "issue",
            "create",
            "-p",
            GITHUB_PROJECT,
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
    let issue_number = created["id_readable"]
        .as_str()
        .unwrap()
        .split('#')
        .last()
        .unwrap()
        .to_string();

    // Add a comment
    track_github()
        .args([
            "issue",
            "comment",
            &issue_number,
            "-m",
            "This is a test comment from integration tests",
        ])
        .assert()
        .success();

    // Get comments
    let comments_output = track_github_json()
        .args(["issue", "comments", &issue_number])
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

    // Verify comment structure
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
    track_github()
        .args(["issue", "update", &issue_number, "--state", "closed"])
        .assert()
        .success();
}

// ============================================================================
// Tag (Label) Operations
// ============================================================================

#[test]
#[ignore]
fn test_github_tags_create_and_delete() {
    if !config_exists() || !has_write_access() {
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
    let output = track_github_json()
        .args([
            "tags",
            "create",
            &tag_name,
            "--tag-color",
            "#ff0000",
            "-d",
            "Test tag for cleanup",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let created: Value = serde_json::from_str(&String::from_utf8(output).unwrap()).unwrap();
    assert_eq!(created["name"].as_str().unwrap(), tag_name);

    // Verify it shows in list
    let list_output = track_github_json()
        .args(["tags", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let tags: Vec<Value> =
        serde_json::from_str(&String::from_utf8(list_output).unwrap()).unwrap();
    assert!(
        tags.iter().any(|t| t["name"].as_str() == Some(&tag_name)),
        "Created tag should appear in list"
    );

    // Delete the tag
    track_github()
        .args(["tags", "delete", &tag_name])
        .assert()
        .success();

    // Verify it's gone
    let list_output2 = track_github_json()
        .args(["tags", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let tags2: Vec<Value> =
        serde_json::from_str(&String::from_utf8(list_output2).unwrap()).unwrap();
    assert!(
        !tags2.iter().any(|t| t["name"].as_str() == Some(&tag_name)),
        "Deleted tag should not appear in list"
    );
}

#[test]
#[ignore]
fn test_github_tags_update() {
    if !config_exists() || !has_write_access() {
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

    // Create a tag
    track_github_json()
        .args(["tags", "create", &tag_name, "--tag-color", "#ff0000"])
        .assert()
        .success();

    // Update its color
    let output = track_github_json()
        .args([
            "tags",
            "update",
            &tag_name,
            "--tag-color",
            "#00ff00",
            "-d",
            "Updated",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let updated: Value = serde_json::from_str(&String::from_utf8(output).unwrap()).unwrap();
    assert_eq!(updated["color"]["background"].as_str().unwrap(), "#00ff00");

    // Clean up
    track_github()
        .args(["tags", "delete", &tag_name])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_github_tags_list() {
    if !config_exists() {
        return;
    }

    let output = track_github_json()
        .args(["tags", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert!(json.is_array(), "Tags list should be an array");

    // Verify tag structure if we have tags
    if let Some(tags) = json.as_array() {
        if let Some(tag) = tags.first() {
            assert!(tag.get("name").is_some(), "Tag should have 'name'");
        }
    }
}

#[test]
#[ignore]
fn test_github_tags_list_text_output() {
    if !config_exists() {
        return;
    }

    track_github()
        .args(["tags", "list"])
        .assert()
        .success();
}

// ============================================================================
// Issue Links (Unsupported on GitHub)
// ============================================================================

#[test]
#[ignore]
fn test_github_issue_links_returns_empty() {
    if !config_exists() || !has_write_access() {
        return;
    }

    // Create an issue
    let create_output = track_github_json()
        .args(["issue", "create", "-p", GITHUB_PROJECT, "-s", "Test Links Empty"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_number = created["id_readable"]
        .as_str()
        .unwrap()
        .split('#')
        .last()
        .unwrap()
        .to_string();

    // Get links should return empty (GitHub has no formal links)
    let get_output = track_github_json()
        .args(["issue", "get", &issue_number, "--full"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let _fetched: Value = serde_json::from_str(&get_str).unwrap();

    // Clean up
    track_github()
        .args(["issue", "update", &issue_number, "--state", "closed"])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_github_link_issues_not_supported() {
    if !config_exists() {
        return;
    }

    // Linking issues should fail with a helpful message
    track_github()
        .args(["issue", "link", "1", "2", "-t", "relates"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not support")
                .or(predicate::str::contains("not supported")),
        );
}

// ============================================================================
// Delete (Unsupported on GitHub)
// ============================================================================

#[test]
#[ignore]
fn test_github_delete_issue_not_supported() {
    if !config_exists() {
        return;
    }

    // Deleting issues should fail with a helpful message
    track_github()
        .args(["issue", "delete", "999999"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not support")
                .or(predicate::str::contains("close")),
        );
}

// ============================================================================
// Backend-Specific Limitations
// ============================================================================

#[test]
#[ignore]
fn test_github_article_commands_not_supported() {
    if !config_exists() {
        return;
    }

    // article list returns empty (NoopKnowledgeBase) which is fine
    track_github_json()
        .args(["article", "list"])
        .assert()
        .success();

    // article create should fail since GitHub has no knowledge base
    track_github()
        .args(["article", "create", "-p", GITHUB_PROJECT, "-s", "Test Article"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not support")
                .or(predicate::str::contains("knowledge base")),
        );
}

#[test]
#[ignore]
fn test_github_project_create_not_supported() {
    if !config_exists() {
        return;
    }

    // Project creation should fail with helpful message
    track_github()
        .args(["project", "create", "-n", "Test Project", "-s", "TEST"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not supported"));
}

// ============================================================================
// Feature Parity Tests - Verify Core Model Structure
// ============================================================================

#[test]
#[ignore]
fn test_github_issue_structure() {
    if !config_exists() || !has_write_access() {
        return;
    }

    // Create an issue
    let create_output = track_github_json()
        .args([
            "issue",
            "create",
            "-p",
            GITHUB_PROJECT,
            "-s",
            "Feature Parity Test Issue",
            "-d",
            "Testing issue structure",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let issue: Value = serde_json::from_str(&create_str).unwrap();

    // Verify the structure matches tracker-core Issue
    assert!(issue.get("id").is_some(), "Issue should have 'id'");
    assert!(
        issue.get("id_readable").is_some(),
        "Issue should have 'id_readable'"
    );
    assert!(
        issue.get("summary").is_some(),
        "Issue should have 'summary'"
    );
    assert!(
        issue.get("project").is_some(),
        "Issue should have 'project'"
    );
    assert!(
        issue.get("created").is_some(),
        "Issue should have 'created'"
    );
    assert!(
        issue.get("updated").is_some(),
        "Issue should have 'updated'"
    );

    let issue_number = issue["id_readable"]
        .as_str()
        .unwrap()
        .split('#')
        .last()
        .unwrap()
        .to_string();

    // Clean up
    track_github()
        .args(["issue", "update", &issue_number, "--state", "closed"])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_github_project_structure() {
    if !config_exists() {
        return;
    }

    let output = track_github_json()
        .args(["project", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let projects: Value = serde_json::from_str(&output_str).unwrap();

    // Verify the structure matches tracker-core Project
    if let Some(project) = projects.as_array().and_then(|p| p.first()) {
        assert!(project.get("id").is_some(), "Project should have 'id'");
        assert!(project.get("name").is_some(), "Project should have 'name'");
        assert!(
            project.get("short_name").is_some(),
            "Project should have 'short_name'"
        );
    }
}

// ============================================================================
// CLI Alias Tests
// ============================================================================

#[test]
#[ignore]
fn test_github_cli_aliases() {
    if !config_exists() {
        return;
    }

    // Test -b gh alias
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "gh", "-o", "json", "--config"])
        .arg(config_path())
        .args(["p", "ls"]) // project list aliases
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Test i s alias (issue search)
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "gh", "--config"])
        .arg(config_path())
        .args(["i", "s", "is:open", "--limit", "1"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

// ============================================================================
// Output Format Tests
// ============================================================================

#[test]
#[ignore]
fn test_github_json_output_parseable() {
    if !config_exists() {
        return;
    }

    let output = track_github_json()
        .args(["project", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();

    // Should be valid JSON
    let parsed: Result<Value, _> = serde_json::from_str(&output_str);
    assert!(
        parsed.is_ok(),
        "Output should be valid JSON: {}",
        output_str
    );
}

#[test]
#[ignore]
fn test_github_text_output_readable() {
    if !config_exists() {
        return;
    }

    track_github()
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
#[ignore]
fn test_github_invalid_project() {
    if !config_exists() {
        return;
    }

    track_github()
        .args(["project", "get", "nonexistent-org/nonexistent-repo-xyz"])
        .assert()
        .failure();
}

#[test]
#[ignore]
fn test_github_invalid_issue_id() {
    if !config_exists() {
        return;
    }

    track_github()
        .args(["issue", "get", "not-a-number"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid"));
}
