//! Integration tests for Jira backend
//!
//! These tests run against a real Jira instance and are ignored by default.
//! To run them:
//!   cargo test --package track --test jira_integration_tests -- --ignored
//!
//! Prerequisites:
//!   - Ensure .track.toml in the project root contains a [jira] section with:
//!     [jira]
//!     url = "https://your-domain.atlassian.net"
//!     email = "your-email@example.com"
//!     token = "your-api-token"
//!
//!   - Have at least one project in Jira (e.g., "SMS")

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Get the path to the .track.toml config file at project root
fn jira_config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".track.toml")
}

/// Check if the config file exists (skip tests if not)
fn config_exists() -> bool {
    jira_config_path().exists()
}

// ============================================================================
// Connection & Configuration Tests
// ============================================================================

#[test]
#[ignore]
fn test_jira_config_test_command() {
    if !config_exists() {
        eprintln!("Skipping: .track.toml not found");
        return;
    }

    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["config", "test"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("Connected to"));
}

#[test]
#[ignore]
fn test_jira_missing_email_error() {
    // Test that missing email gives a clear error
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "project", "list"])
        .env("JIRA_URL", "https://test.atlassian.net")
        .env("JIRA_TOKEN", "test-token")
        .env_remove("JIRA_EMAIL")
        .timeout(Duration::from_secs(10))
        .assert()
        .failure()
        .stderr(predicate::str::contains("email not configured"));
}

// ============================================================================
// Project Operations
// ============================================================================

#[test]
#[ignore]
fn test_jira_project_list() {
    if !config_exists() {
        return;
    }

    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["project", "list"])
        .timeout(Duration::from_secs(30))
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
fn test_jira_project_list_text_output() {
    if !config_exists() {
        return;
    }

    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["project", "list"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("SMS").or(predicate::str::contains("MBA")));
}

#[test]
#[ignore]
fn test_jira_project_get() {
    if !config_exists() {
        return;
    }

    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["project", "get", "SMS"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let json: Value = serde_json::from_str(&output_str).unwrap();

    assert_eq!(json["short_name"], "SMS");
    assert!(json["name"].is_string());
}

// ============================================================================
// Issue Search Operations
// ============================================================================

#[test]
#[ignore]
fn test_jira_issue_search_jql() {
    if !config_exists() {
        return;
    }

    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "search", "project = SMS", "--limit", "5"])
        .timeout(Duration::from_secs(30))
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
fn test_jira_issue_search_with_status() {
    if !config_exists() {
        return;
    }

    // Test JQL with status filter
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "search",
            "project = SMS AND status = Open",
            "--limit",
            "5",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_jira_issue_search_unresolved() {
    if !config_exists() {
        return;
    }

    // Test JQL for unresolved issues
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "search",
            "project = SMS AND resolution IS EMPTY",
            "--limit",
            "5",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_jira_issue_search_pagination() {
    if !config_exists() {
        return;
    }

    // Test pagination with skip
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "search",
            "project = SMS",
            "--limit",
            "2",
            "--skip",
            "0",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

// ============================================================================
// Issue CRUD Operations
// ============================================================================

#[test]
#[ignore]
fn test_jira_issue_create_and_delete() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Integration Test Issue - DELETE ME",
            "-d",
            "This is an automated test issue created by integration tests.",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();

    let issue_key = created["id_readable"].as_str().unwrap();
    assert!(
        issue_key.starts_with("SMS-"),
        "Issue key should start with SMS-"
    );

    // Get the issue we just created
    let get_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "get", issue_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    assert_eq!(fetched["summary"], "Integration Test Issue - DELETE ME");

    // Delete the issue
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "delete", issue_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Verify deletion - should fail to get
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "get", issue_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .failure();
}

#[test]
#[ignore]
fn test_jira_issue_update() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Test Issue for Update",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_key = created["id_readable"].as_str().unwrap().to_string();

    // Update the issue
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "update",
            &issue_key,
            "--summary",
            "Updated Test Issue Summary",
            "--description",
            "Updated description",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Verify update
    let get_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "get", &issue_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    assert_eq!(fetched["summary"], "Updated Test Issue Summary");

    // Clean up
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "delete", &issue_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

// ============================================================================
// Issue Get Operations
// ============================================================================

#[test]
#[ignore]
fn test_jira_issue_get_shortcut() {
    if !config_exists() {
        return;
    }

    // First create an issue to have something to get
    let create_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "-p", "SMS", "-s", "Test Shortcut Access"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_key = created["id_readable"].as_str().unwrap().to_string();

    // Test direct access shortcut (track SMS-123 instead of track issue get SMS-123)
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .arg(&issue_key)
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains(&issue_key))
        .stdout(predicate::str::contains("Test Shortcut Access"));

    // Clean up
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "delete", &issue_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_jira_issue_get_not_found() {
    if !config_exists() {
        return;
    }

    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "get", "SMS-99999"])
        .timeout(Duration::from_secs(30))
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Not found").or(predicate::str::contains("does not exist")),
        );
}

// ============================================================================
// Comment Operations
// ============================================================================

#[test]
#[ignore]
fn test_jira_issue_comments() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Test Issue for Comments",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_key = created["id_readable"].as_str().unwrap().to_string();

    // Add a comment
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "comment",
            &issue_key,
            "-m",
            "This is a test comment from integration tests",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Get comments
    let comments_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "comments", &issue_key])
        .timeout(Duration::from_secs(30))
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
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "delete", &issue_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

// ============================================================================
// Issue Link Operations
// ============================================================================

#[test]
#[ignore]
fn test_jira_issue_link() {
    if !config_exists() {
        return;
    }

    // Create two issues
    let create1 = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "-p", "SMS", "-s", "Link Test Issue 1"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create2 = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "-p", "SMS", "-s", "Link Test Issue 2"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue1: Value = serde_json::from_str(&String::from_utf8(create1).unwrap()).unwrap();
    let issue2: Value = serde_json::from_str(&String::from_utf8(create2).unwrap()).unwrap();
    let key1 = issue1["id_readable"].as_str().unwrap().to_string();
    let key2 = issue2["id_readable"].as_str().unwrap().to_string();

    // Link the issues
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "link", &key1, &key2, "-t", "Relates"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Clean up
    for key in [&key1, &key2] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

#[test]
#[ignore]
fn test_jira_issue_link_subtask() {
    if !config_exists() {
        return;
    }

    // Create parent as Epic (Jira requires parent to be Epic/Story for subtask hierarchy)
    let create_parent = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Subtask Link Parent",
            "-f",
            "Type=Epic",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_child = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "-p", "SMS", "-s", "Subtask Link Child"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parent: Value = serde_json::from_str(&String::from_utf8(create_parent).unwrap()).unwrap();
    let child: Value = serde_json::from_str(&String::from_utf8(create_child).unwrap()).unwrap();
    let parent_key = parent["id_readable"].as_str().unwrap().to_string();
    let child_key = child["id_readable"].as_str().unwrap().to_string();

    // Link child as subtask of parent: issue link CHILD PARENT -t subtask
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "link", &child_key, &parent_key, "-t", "subtask"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Clean up
    for key in [&child_key, &parent_key] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

#[test]
#[ignore]
fn test_jira_issue_link_parent() {
    if !config_exists() {
        return;
    }

    // Create parent as Epic (Jira requires parent to be Epic/Story for subtask hierarchy)
    let create_parent = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Parent Link Parent",
            "-f",
            "Type=Epic",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_child = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "-p", "SMS", "-s", "Parent Link Child"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parent: Value = serde_json::from_str(&String::from_utf8(create_parent).unwrap()).unwrap();
    let child: Value = serde_json::from_str(&String::from_utf8(create_child).unwrap()).unwrap();
    let parent_key = parent["id_readable"].as_str().unwrap().to_string();
    let child_key = child["id_readable"].as_str().unwrap().to_string();

    // Link parent as parent of child: issue link PARENT CHILD -t parent
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "link", &parent_key, &child_key, "-t", "parent"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Clean up
    for key in [&child_key, &parent_key] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

// ============================================================================
// Feature Parity Tests - Compare YouTrack and Jira Behavior
// ============================================================================

/// Test that project list returns similar structure for both backends
#[test]
#[ignore]
fn test_feature_parity_project_structure() {
    if !config_exists() {
        return;
    }

    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["project", "list"])
        .timeout(Duration::from_secs(30))
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

/// Test that issue structure is consistent
#[test]
#[ignore]
fn test_feature_parity_issue_structure() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Feature Parity Test Issue",
            "-d",
            "Testing issue structure",
        ])
        .timeout(Duration::from_secs(30))
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

    let issue_key = issue["id_readable"].as_str().unwrap();

    // Clean up
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "delete", issue_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

/// Test that comment structure is consistent
#[test]
#[ignore]
fn test_feature_parity_comment_structure() {
    if !config_exists() {
        return;
    }

    // Create an issue and add a comment
    let create_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "-p", "SMS", "-s", "Comment Parity Test"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let issue: Value = serde_json::from_str(&create_str).unwrap();
    let issue_key = issue["id_readable"].as_str().unwrap().to_string();

    // Add comment
    let comment_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "comment",
            &issue_key,
            "-m",
            "Test comment for structure verification",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let comment_str = String::from_utf8(comment_output).unwrap();
    let comment: Value = serde_json::from_str(&comment_str).unwrap();

    // Verify the structure matches tracker-core Comment
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
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "delete", &issue_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
#[ignore]
fn test_jira_invalid_project() {
    if !config_exists() {
        return;
    }

    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["project", "get", "NONEXISTENT_PROJECT_XYZ"])
        .timeout(Duration::from_secs(30))
        .assert()
        .failure();
}

#[test]
#[ignore]
fn test_jira_invalid_jql() {
    if !config_exists() {
        return;
    }

    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "search", "INVALID JQL SYNTAX ==="])
        .timeout(Duration::from_secs(30))
        .assert()
        .failure();
}

// ============================================================================
// Parent Field Operations
// ============================================================================

#[test]
#[ignore]
fn test_jira_issue_create_with_parent() {
    if !config_exists() {
        return;
    }

    // Create a parent issue as an Epic (Jira hierarchy: Epic → Task → Subtask)
    let parent_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Parent Epic for Test",
            "-f",
            "Type=Epic",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parent: Value = serde_json::from_str(&String::from_utf8(parent_output).unwrap()).unwrap();
    let parent_key = parent["id_readable"].as_str().unwrap().to_string();

    // Create a child Task with --parent pointing to the Epic
    let child_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Child Task with Parent",
            "--parent",
            &parent_key,
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let child: Value = serde_json::from_str(&String::from_utf8(child_output).unwrap()).unwrap();
    let child_key = child["id_readable"].as_str().unwrap().to_string();
    assert!(
        child_key.starts_with("SMS-"),
        "Child issue should be created successfully"
    );

    // Clean up both issues
    for key in [&child_key, &parent_key] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

#[test]
#[ignore]
fn test_jira_issue_update_with_parent() {
    if !config_exists() {
        return;
    }

    // Create a parent Epic
    let parent_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Parent Epic for Update Test",
            "-f",
            "Type=Epic",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parent: Value = serde_json::from_str(&String::from_utf8(parent_output).unwrap()).unwrap();
    let parent_key = parent["id_readable"].as_str().unwrap().to_string();

    // Create a standalone Task (no parent)
    let child_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "-p", "SMS", "-s", "Standalone Task"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let child: Value = serde_json::from_str(&String::from_utf8(child_output).unwrap()).unwrap();
    let child_key = child["id_readable"].as_str().unwrap().to_string();

    // Update the Task to set --parent to the Epic
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "update", &child_key, "--parent", &parent_key])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Clean up both issues
    for key in [&child_key, &parent_key] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

#[test]
#[ignore]
fn test_jira_issue_create_with_parent_json() {
    if !config_exists() {
        return;
    }

    // Create a parent Epic
    let parent_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Parent Epic for JSON Test",
            "-f",
            "Type=Epic",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parent: Value = serde_json::from_str(&String::from_utf8(parent_output).unwrap()).unwrap();
    let parent_key = parent["id_readable"].as_str().unwrap().to_string();

    // Create a child Task via --json payload with parent field
    let json_payload = format!(
        r#"{{"project": "SMS", "summary": "JSON Child Task", "parent": "{}"}}"#,
        parent_key
    );

    let child_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "--json", &json_payload])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let child: Value = serde_json::from_str(&String::from_utf8(child_output).unwrap()).unwrap();
    let child_key = child["id_readable"].as_str().unwrap().to_string();
    assert!(
        child_key.starts_with("SMS-"),
        "JSON-created child issue should succeed"
    );

    // Clean up both issues
    for key in [&child_key, &parent_key] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

#[test]
#[ignore]
fn test_jira_issue_update_with_parent_json() {
    if !config_exists() {
        return;
    }

    // Create a parent Epic
    let parent_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Parent Epic for JSON Update Test",
            "-f",
            "Type=Epic",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parent: Value = serde_json::from_str(&String::from_utf8(parent_output).unwrap()).unwrap();
    let parent_key = parent["id_readable"].as_str().unwrap().to_string();

    // Create a standalone Task
    let child_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Standalone Task for JSON Update",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let child: Value = serde_json::from_str(&String::from_utf8(child_output).unwrap()).unwrap();
    let child_key = child["id_readable"].as_str().unwrap().to_string();

    // Update the Task via --json to set parent to the Epic
    let json_payload = format!(r#"{{"parent": "{}"}}"#, parent_key);

    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "update", &child_key, "--json", &json_payload])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Clean up both issues
    for key in [&child_key, &parent_key] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

#[test]
#[ignore]
fn test_jira_issue_create_subtask_with_parent() {
    if !config_exists() {
        return;
    }

    // Create a parent Task (in Jira: Task → Subtask hierarchy)
    let parent_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Parent Task for Subtask Test",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let parent: Value = serde_json::from_str(&String::from_utf8(parent_output).unwrap()).unwrap();
    let parent_key = parent["id_readable"].as_str().unwrap().to_string();

    // Create a Subtask with --parent pointing to the Task
    let child_output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Subtask under Parent Task",
            "-f",
            "Type=Subtask",
            "--parent",
            &parent_key,
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let child: Value = serde_json::from_str(&String::from_utf8(child_output).unwrap()).unwrap();
    let child_key = child["id_readable"].as_str().unwrap().to_string();
    assert!(
        child_key.starts_with("SMS-"),
        "Subtask should be created successfully under parent Task"
    );

    // Verify the subtask's Type is Subtask
    let type_field = child["custom_fields"].as_array().and_then(|fields| {
        fields.iter().find(|f| {
            f.get("SingleEnum")
                .and_then(|e| e["name"].as_str())
                .is_some_and(|n| n == "Type")
        })
    });
    if let Some(field) = type_field {
        assert_eq!(
            field["SingleEnum"]["value"].as_str().unwrap(),
            "Subtask",
            "Child issue type should be Subtask"
        );
    }

    // Clean up both issues (child first, then parent)
    for key in [&child_key, &parent_key] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

// ============================================================================
// Backend-Specific Limitations
// ============================================================================

#[test]
#[ignore]
fn test_jira_article_list_via_confluence() {
    if !config_exists() {
        return;
    }

    // Jira article commands are backed by Confluence
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["article", "list"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_jira_project_create_not_supported() {
    if !config_exists() {
        return;
    }

    // Project creation should fail with helpful message
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["project", "create", "-n", "Test Project", "-s", "TEST"])
        .timeout(Duration::from_secs(30))
        .assert()
        .failure()
        .stderr(predicate::str::contains("not supported"));
}

// ============================================================================
// CLI Alias Tests (Backend-Agnostic)
// ============================================================================

#[test]
#[ignore]
fn test_jira_cli_aliases() {
    if !config_exists() {
        return;
    }

    // Test -b j alias
    cargo_bin_cmd!("track")
        .args(["-b", "j", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["p", "ls"]) // project list aliases
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Test i s alias (issue search)
    cargo_bin_cmd!("track")
        .args(["-b", "j", "--config"])
        .arg(jira_config_path())
        .args(["i", "s", "project = SMS", "--limit", "1"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

// ============================================================================
// Output Format Tests
// ============================================================================

#[test]
#[ignore]
fn test_jira_json_output_parseable() {
    if !config_exists() {
        return;
    }

    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["project", "list"])
        .timeout(Duration::from_secs(30))
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
fn test_jira_text_output_readable() {
    if !config_exists() {
        return;
    }

    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["project", "list"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("(")) // Format: "Name (KEY) - Description"
        .stdout(predicate::str::contains(")"));
}

// ============================================================================
// Pagination Hint Tests (with mock server)
// ============================================================================

/// Helper to start a mock server for Jira tests.
fn start_jira_mock_server(response_body: String) -> (thread::JoinHandle<()>, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();

    let handle = thread::spawn(move || {
        use std::io::{Read, Write};

        let timeout = Duration::from_secs(3);
        for stream in listener.incoming().flatten().take(3) {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(timeout));
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

    (handle, port)
}

fn mock_jira_issue(key: &str, summary: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "10001",
        "key": key,
        "self": format!("https://test.atlassian.net/rest/api/3/issue/{}", key),
        "fields": {
            "summary": summary,
            "description": null,
            "issuetype": {"name": "Task", "id": "10001"},
            "status": {"name": "Open", "statusCategory": {"key": "new"}},
            "project": {"id": "10000", "key": "TEST", "name": "Test Project"},
            "priority": {"name": "Medium", "id": "3"},
            "assignee": null,
            "created": "2024-01-01T00:00:00.000+0000",
            "updated": "2024-01-02T00:00:00.000+0000",
            "labels": [],
            "comment": {"comments": [], "total": 0}
        }
    })
}

/// Write a temporary Jira config file and return its path.
fn write_jira_mock_config(port: u16) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("track-jira-test-{}", port));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join(".track.toml");
    let content = format!(
        r#"
[jira]
url = "http://127.0.0.1:{}"
email = "test@test.com"
token = "test"
"#,
        port
    );
    std::fs::write(&config_path, content).unwrap();
    config_path
}

fn write_jira_mock_config_with_credentials(
    port: u16,
    email: &str,
    token: &str,
) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("track-jira-test-auth-{}", port));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join(".track.toml");
    let content = format!(
        r#"
[jira]
url = "http://127.0.0.1:{}"
email = {:?}
token = {:?}
"#,
        port, email, token
    );
    std::fs::write(&config_path, content).unwrap();
    config_path
}

fn start_jira_auth_capture_server(
    response_body: String,
) -> (thread::JoinHandle<()>, u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();
    let captured_headers = Arc::new(Mutex::new(Vec::new()));
    let captured_headers_for_thread = Arc::clone(&captured_headers);

    let handle = thread::spawn(move || {
        use std::io::{Read, Write};

        let timeout = Duration::from_secs(3);
        if let Some(stream) = listener.incoming().flatten().next() {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(timeout));
            let mut buffer = [0; 4096];
            if let Ok(size) = stream.read(&mut buffer) {
                let request = String::from_utf8_lossy(&buffer[..size]);
                if let Some(auth_header) = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_string())
                }) {
                    captured_headers_for_thread
                        .lock()
                        .unwrap()
                        .push(auth_header);
                }

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
        }
    });

    (handle, port, captured_headers)
}

#[test]
fn test_jira_pagination_hint_on_full_page() {
    // Jira search response includes total inline
    let search_response = serde_json::json!({
        "startAt": 0,
        "maxResults": 2,
        "total": 10,
        "issues": [
            mock_jira_issue("TEST-1", "Issue 1"),
            mock_jira_issue("TEST-2", "Issue 2")
        ]
    });

    let (_server, port) = start_jira_mock_server(search_response.to_string());
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config(port);
    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(&cfg)
        .args(["issue", "search", "project = TEST", "--limit", "2"])
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
}

#[test]
fn test_jira_cli_trims_whitespace_in_basic_auth_header() {
    let issue_response = mock_jira_issue("TEST-123", "Whitespace auth proof").to_string();
    let (server, port, captured_headers) = start_jira_auth_capture_server(issue_response);
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config_with_credentials(
        port,
        " test@test.com \n",
        "\rFAKE-TOKEN-DO-NOT-USE\t ",
    );

    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(&cfg)
        .args(["issue", "get", "TEST-123"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success();

    server.join().unwrap();

    let headers = captured_headers.lock().unwrap();
    assert_eq!(
        headers.len(),
        1,
        "Expected one Jira request through the CLI path"
    );
    assert_eq!(
        headers[0], "Basic dGVzdEB0ZXN0LmNvbTpGQUtFLVRPS0VOLURPLU5PVC1VU0U=",
        "CLI should send the trimmed Jira Basic auth header"
    );
}

#[test]
fn test_jira_no_hint_on_partial_page() {
    let search_response = serde_json::json!({
        "startAt": 0,
        "maxResults": 10,
        "total": 2,
        "issues": [
            mock_jira_issue("TEST-1", "Issue 1"),
            mock_jira_issue("TEST-2", "Issue 2")
        ]
    });

    let (_server, port) = start_jira_mock_server(search_response.to_string());
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config(port);
    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(&cfg)
        .args(["issue", "search", "project = TEST", "--limit", "10"])
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

// ============================================================================
// Unlink Tests
// ============================================================================

#[test]
fn test_jira_unlink_text_output() {
    // Jira returns 204 No Content on delete link — mock returns 200 which is also fine
    let mock_response = String::new();
    let (_server, port) = start_jira_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config(port);
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(&cfg)
        .args(["issue", "unlink", "PROJ-123", "12345"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(predicate::str::contains("PROJ-123"))
        .stdout(predicate::str::contains("unlinked"))
        .stdout(predicate::str::contains("12345"));
}

#[test]
fn test_jira_unlink_json_output() {
    let mock_response = String::new();
    let (_server, port) = start_jira_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config(port);
    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(&cfg)
        .args(["issue", "unlink", "PROJ-123", "12345"])
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
    assert_eq!(json["linkId"], "12345");
}

// ============================================================================
// Link Type Mapping Tests (mock-based)
// ============================================================================

#[test]
fn test_jira_link_depends_text_output() {
    // Jira returns 201 Created on POST /rest/api/3/issueLink
    let mock_response = String::new();
    let (_server, port) = start_jira_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config(port);
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(&cfg)
        .args(["issue", "link", "PROJ-1", "PROJ-2", "-t", "depends"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(predicate::str::contains("PROJ-1"))
        .stdout(predicate::str::contains("depends on"))
        .stdout(predicate::str::contains("PROJ-2"));
}

#[test]
fn test_jira_link_depends_json_output() {
    let mock_response = String::new();
    let (_server, port) = start_jira_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config(port);
    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(&cfg)
        .args(["issue", "link", "PROJ-1", "PROJ-2", "-t", "depends"])
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
fn test_jira_link_relates_text_output() {
    let mock_response = String::new();
    let (_server, port) = start_jira_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config(port);
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(&cfg)
        .args(["issue", "link", "PROJ-1", "PROJ-2", "-t", "relates"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .stdout(predicate::str::contains("relates to"));
}

#[test]
fn test_jira_link_duplicates_json_output() {
    let mock_response = String::new();
    let (_server, port) = start_jira_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config(port);
    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(&cfg)
        .args(["issue", "link", "PROJ-1", "PROJ-2", "-t", "duplicates"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_str(&String::from_utf8(output).unwrap()).unwrap();
    assert_eq!(json["linkType"], "duplicates");
    assert_eq!(json["description"], "duplicates");
}

#[test]
fn test_jira_link_custom_type_passthrough() {
    // Custom/unknown link types should pass through to the backend
    let mock_response = String::new();
    let (_server, port) = start_jira_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config(port);
    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(&cfg)
        .args(["issue", "link", "PROJ-1", "PROJ-2", "-t", "clones"])
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

/// Write a Jira mock config with custom link_mappings section.
fn write_jira_mock_config_with_mappings(port: u16) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("track-jira-link-map-test-{}", port));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join(".track.toml");
    let content = format!(
        r#"
[jira]
url = "http://127.0.0.1:{}"
email = "test@test.com"
token = "test"

[jira.link_mappings]
depends = "Requires"
"#,
        port
    );
    std::fs::write(&config_path, content).unwrap();
    config_path
}

#[test]
fn test_jira_link_with_config_mappings() {
    // Even with custom mappings, the CLI-level behavior is the same
    // (the mapping happens inside the backend, transparent to the user)
    let mock_response = String::new();
    let (_server, port) = start_jira_mock_server(mock_response);
    thread::sleep(Duration::from_millis(50));

    let cfg = write_jira_mock_config_with_mappings(port);
    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(&cfg)
        .args(["issue", "link", "PROJ-1", "PROJ-2", "-t", "depends"])
        .timeout(Duration::from_secs(5))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_str(&String::from_utf8(output).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["linkType"], "depends");
    assert_eq!(json["description"], "depends on");
}

// ============================================================================
// Live Link Type Mapping Tests
// ============================================================================

#[test]
#[ignore]
fn test_jira_live_link_depends() {
    if !config_exists() {
        return;
    }

    // Create two issues
    let create1 = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "-p", "SMS", "-s", "Depends Link Test 1"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create2 = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["issue", "create", "-p", "SMS", "-s", "Depends Link Test 2"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue1: Value = serde_json::from_str(&String::from_utf8(create1).unwrap()).unwrap();
    let issue2: Value = serde_json::from_str(&String::from_utf8(create2).unwrap()).unwrap();
    let key1 = issue1["id_readable"].as_str().unwrap().to_string();
    let key2 = issue2["id_readable"].as_str().unwrap().to_string();

    // Link with "depends" — should create a "Blocks" link (via config or default mapping)
    // The config has [jira.link_mappings] depends = "Blocks", confirming the mapping is used
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "link", &key1, &key2, "-t", "depends"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("depends on"));

    // Clean up
    for key in [&key1, &key2] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

#[test]
#[ignore]
fn test_jira_live_link_duplicates() {
    if !config_exists() {
        return;
    }

    // Create two issues
    let create1 = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Duplicate Link Test 1",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create2 = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args([
            "issue",
            "create",
            "-p",
            "SMS",
            "-s",
            "Duplicate Link Test 2",
        ])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue1: Value = serde_json::from_str(&String::from_utf8(create1).unwrap()).unwrap();
    let issue2: Value = serde_json::from_str(&String::from_utf8(create2).unwrap()).unwrap();
    let key1 = issue1["id_readable"].as_str().unwrap().to_string();
    let key2 = issue2["id_readable"].as_str().unwrap().to_string();

    // Link with "duplicates" — should create a "Duplicate" link
    cargo_bin_cmd!("track")
        .args(["-b", "jira", "--config"])
        .arg(jira_config_path())
        .args(["issue", "link", &key1, &key2, "-t", "duplicates"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicates"));

    // Clean up
    for key in [&key1, &key2] {
        cargo_bin_cmd!("track")
            .args(["-b", "jira", "--config"])
            .arg(jira_config_path())
            .args(["issue", "delete", key])
            .timeout(Duration::from_secs(30))
            .assert()
            .success();
    }
}

// ============================================================================
// Field Discovery
// ============================================================================

#[test]
#[ignore]
fn test_jira_project_fields_includes_custom_fields() {
    if !config_exists() {
        return;
    }

    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["project", "fields", "SMS"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let fields: Vec<Value> = serde_json::from_str(&output_str).unwrap();

    // Should have standard fields
    let names: Vec<&str> = fields.iter().filter_map(|f| f["name"].as_str()).collect();
    assert!(names.contains(&"Priority"), "should contain Priority");
    assert!(names.contains(&"Status"), "should contain Status");

    // Should also contain instance-level custom fields (from /rest/api/3/field)
    // At minimum, there should be more than just the 5 hardcoded standard fields
    assert!(
        fields.len() > 5,
        "expected more than 5 fields (got {}), instance-level custom fields should be included",
        fields.len()
    );
}

#[test]
#[ignore]
fn test_jira_project_fields_shows_field_ids() {
    if !config_exists() {
        return;
    }

    let output = cargo_bin_cmd!("track")
        .args(["-b", "jira", "-o", "json", "--config"])
        .arg(jira_config_path())
        .args(["project", "fields", "SMS"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let fields: Vec<Value> = serde_json::from_str(&output_str).unwrap();

    // Custom fields should have customfield_ IDs
    let has_custom_ids = fields.iter().any(|f| {
        f["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("customfield_"))
    });
    assert!(
        has_custom_ids,
        "expected at least one field with a customfield_ ID"
    );
}
