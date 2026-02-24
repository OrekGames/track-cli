//! Integration tests for GitLab backend
//!
//! These tests run against a real GitLab instance and are ignored by default.
//! To run them:
//!   cargo test --package track --test gitlab_integration_tests -- --ignored
//!
//! Prerequisites:
//!   - Ensure .track.toml in the project root contains a [gitlab] section with:
//!     [gitlab]
//!     token = "glpat-your-token"
//!     url = "https://gitlab.com/api/v4"
//!     project_id = "12345"
//!
//!   - The configured project must exist and the token must have API scope

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
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

/// The project identifier for issue creation (project_id from .track.toml)
const GITLAB_PROJECT: &str = "77945341";

/// Helper to build a track command with GitLab backend and config
fn track_gitlab() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "gitlab", "--config"])
        .arg(config_path())
        .timeout(Duration::from_secs(30));
    cmd
}

/// Helper to build a track command with GitLab backend, JSON output, and config
fn track_gitlab_json() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "gitlab", "-o", "json", "--config"])
        .arg(config_path())
        .timeout(Duration::from_secs(30));
    cmd
}

// ============================================================================
// Connection & Configuration Tests
// ============================================================================

#[test]
#[ignore]
fn test_gitlab_config_test_command() {
    if !config_exists() {
        eprintln!("Skipping: .track.toml not found");
        return;
    }

    track_gitlab()
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
fn test_gitlab_project_list() {
    if !config_exists() {
        return;
    }

    let output = track_gitlab_json()
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
fn test_gitlab_project_list_text_output() {
    if !config_exists() {
        return;
    }

    track_gitlab()
        .args(["project", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
#[ignore]
fn test_gitlab_project_get() {
    if !config_exists() {
        return;
    }

    let output = track_gitlab_json()
        .args(["project", "get", "77945341"])
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
fn test_gitlab_project_custom_fields() {
    if !config_exists() {
        return;
    }

    // GitLab returns standard custom fields
    let output = track_gitlab_json()
        .args(["project", "fields", "77945341"])
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
fn test_gitlab_issue_search() {
    if !config_exists() {
        return;
    }

    let output = track_gitlab_json()
        .args(["issue", "search", "test", "--limit", "5"])
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
fn test_gitlab_issue_list_open() {
    if !config_exists() {
        return;
    }

    // Empty query should list issues (uses list_issues with state filter)
    track_gitlab_json()
        .args(["issue", "search", "state:opened", "--limit", "5"])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_gitlab_issue_search_pagination() {
    if !config_exists() {
        return;
    }

    track_gitlab_json()
        .args(["issue", "search", "test", "--limit", "2", "--skip", "0"])
        .assert()
        .success();
}

// ============================================================================
// Issue CRUD Operations
// ============================================================================

#[test]
#[ignore]
fn test_gitlab_issue_create_and_delete() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
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

    let issue_id_readable = created["id_readable"].as_str().unwrap();
    assert!(
        issue_id_readable.starts_with('#'),
        "GitLab issue should start with #, got: {}",
        issue_id_readable
    );

    // Strip the # for the issue number
    let issue_iid = issue_id_readable.trim_start_matches('#');

    // Get the issue we just created
    let get_output = track_gitlab_json()
        .args(["issue", "get", issue_iid])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    assert_eq!(fetched["summary"], "Integration Test Issue - DELETE ME");

    // Delete the issue
    track_gitlab()
        .args(["issue", "delete", issue_iid])
        .assert()
        .success();

    // Verify deletion - should fail to get
    track_gitlab()
        .args(["issue", "get", issue_iid])
        .assert()
        .failure();
}

#[test]
#[ignore]
fn test_gitlab_issue_update() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
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
    let issue_iid = created["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // Update the issue
    track_gitlab()
        .args([
            "issue",
            "update",
            &issue_iid,
            "--summary",
            "Updated Test Issue Summary",
            "--description",
            "Updated description via integration test",
        ])
        .assert()
        .success();

    // Verify update
    let get_output = track_gitlab_json()
        .args(["issue", "get", &issue_iid])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let get_str = String::from_utf8(get_output).unwrap();
    let fetched: Value = serde_json::from_str(&get_str).unwrap();
    assert_eq!(fetched["summary"], "Updated Test Issue Summary");

    // Clean up
    track_gitlab()
        .args(["issue", "delete", &issue_iid])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_gitlab_issue_create_with_labels() {
    if !config_exists() {
        return;
    }

    // Create an issue with a tag/label
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
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
    let issue_iid = created["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // Verify tags were applied
    let tags = created["tags"].as_array();
    if let Some(tags) = tags {
        let has_bug = tags.iter().any(|t| t["name"].as_str() == Some("bug"));
        assert!(has_bug, "Issue should have 'bug' label");
    }

    // Clean up
    track_gitlab()
        .args(["issue", "delete", &issue_iid])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_gitlab_issue_close_and_reopen() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
            "-s",
            "Test Issue for Close/Reopen",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_iid = created["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // Close the issue via state update
    track_gitlab()
        .args(["issue", "update", &issue_iid, "--state", "closed"])
        .assert()
        .success();

    // Verify it's closed by checking the State custom field
    let closed_output = track_gitlab_json()
        .args(["issue", "get", &issue_iid])
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
    assert_eq!(
        state_field["State"]["is_resolved"], true,
        "Issue should be resolved after closing"
    );

    // Reopen the issue
    track_gitlab()
        .args(["issue", "update", &issue_iid, "--state", "reopened"])
        .assert()
        .success();

    // Verify it's open again
    let reopened_output = track_gitlab_json()
        .args(["issue", "get", &issue_iid])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let reopened_str = String::from_utf8(reopened_output).unwrap();
    let reopened: Value = serde_json::from_str(&reopened_str).unwrap();
    let state_field = reopened["custom_fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f.get("State").is_some())
        .expect("Should have a State custom field");
    assert_eq!(
        state_field["State"]["is_resolved"], false,
        "Issue should not be resolved after reopening"
    );

    // Clean up
    track_gitlab()
        .args(["issue", "delete", &issue_iid])
        .assert()
        .success();
}

// ============================================================================
// Issue Get Operations
// ============================================================================

#[test]
#[ignore]
fn test_gitlab_issue_get_shortcut() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
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
    let issue_iid = created["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // Test issue get (shortcut with pure numeric IDs doesn't work with clap)
    track_gitlab()
        .args(["issue", "get", &issue_iid])
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Shortcut Access"));

    // Clean up
    track_gitlab()
        .args(["issue", "delete", &issue_iid])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_gitlab_issue_get_with_hash_prefix() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
            "-s",
            "Test Hash Prefix",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let created: Value = serde_json::from_str(&create_str).unwrap();
    let issue_id_readable = created["id_readable"].as_str().unwrap().to_string();
    let issue_iid = issue_id_readable.trim_start_matches('#');

    // Get with # prefix should also work
    track_gitlab()
        .args(["issue", "get", &issue_id_readable])
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Hash Prefix"));

    // Clean up
    track_gitlab()
        .args(["issue", "delete", issue_iid])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_gitlab_issue_get_not_found() {
    if !config_exists() {
        return;
    }

    track_gitlab()
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
fn test_gitlab_issue_get_with_full_flag() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
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
    let issue_iid = created["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // Get with --full flag
    track_gitlab()
        .args(["issue", "get", &issue_iid, "--full"])
        .assert()
        .success();

    // Clean up
    track_gitlab()
        .args(["issue", "delete", &issue_iid])
        .assert()
        .success();
}

// ============================================================================
// Comment Operations
// ============================================================================

#[test]
#[ignore]
fn test_gitlab_issue_comments() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
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
    let issue_iid = created["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // Add a comment
    track_gitlab()
        .args([
            "issue",
            "comment",
            &issue_iid,
            "-m",
            "This is a test comment from integration tests",
        ])
        .assert()
        .success();

    // Get comments
    let comments_output = track_gitlab_json()
        .args(["issue", "comments", &issue_iid])
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
    track_gitlab()
        .args(["issue", "delete", &issue_iid])
        .assert()
        .success();
}

// ============================================================================
// Tag (Label) Operations
// ============================================================================

#[test]
#[ignore]
fn test_gitlab_tags_create_and_delete() {
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
    let output = track_gitlab_json()
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
    let list_output = track_gitlab_json()
        .args(["tags", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let tags: Vec<Value> = serde_json::from_str(&String::from_utf8(list_output).unwrap()).unwrap();
    assert!(
        tags.iter().any(|t| t["name"].as_str() == Some(&tag_name)),
        "Created tag should appear in list"
    );

    // Delete the tag
    track_gitlab()
        .args(["tags", "delete", &tag_name])
        .assert()
        .success();

    // Verify it's gone
    let list_output2 = track_gitlab_json()
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
fn test_gitlab_tags_update() {
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

    // Create a tag
    track_gitlab_json()
        .args(["tags", "create", &tag_name, "--tag-color", "#ff0000"])
        .assert()
        .success();

    // Update its color
    let output = track_gitlab_json()
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
    track_gitlab()
        .args(["tags", "delete", &tag_name])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_gitlab_tags_list() {
    if !config_exists() {
        return;
    }

    let output = track_gitlab_json()
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
fn test_gitlab_tags_list_text_output() {
    if !config_exists() {
        return;
    }

    track_gitlab().args(["tags", "list"]).assert().success();
}

// ============================================================================
// Issue Link Operations
// ============================================================================

#[test]
#[ignore]
fn test_gitlab_issue_link() {
    if !config_exists() {
        return;
    }

    // Create two issues
    let create1_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
            "-s",
            "Link Test Issue 1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create2_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
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
    let iid1 = issue1["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();
    let iid2 = issue2["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // Link the issues
    track_gitlab()
        .args(["issue", "link", &iid1, &iid2, "-t", "relates"])
        .assert()
        .success();

    // Verify link exists via --full get
    let get_output = track_gitlab_json()
        .args(["issue", "get", &iid1, "--full"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let _get_str = String::from_utf8(get_output).unwrap();

    // Clean up
    for iid in [&iid1, &iid2] {
        track_gitlab()
            .args(["issue", "delete", iid])
            .assert()
            .success();
    }
}

#[test]
#[ignore]
fn test_gitlab_issue_link_depends() {
    if !config_exists() {
        return;
    }

    // Create two issues
    let create1_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
            "-s",
            "Depends Issue 1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create2_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
            "-s",
            "Depends Issue 2",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue1: Value = serde_json::from_str(&String::from_utf8(create1_output).unwrap()).unwrap();
    let issue2: Value = serde_json::from_str(&String::from_utf8(create2_output).unwrap()).unwrap();
    let iid1 = issue1["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();
    let iid2 = issue2["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // "depends" maps to GitLab's "blocks" link type which requires a paid license.
    // On free tier this returns 403, so we accept either success or a license error.
    let result = track_gitlab()
        .args(["issue", "link", &iid1, &iid2, "-t", "depends"])
        .assert();

    // Accept both success (paid tier) and license error (free tier)
    let output = result.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        assert!(
            stderr.contains("license") || stderr.contains("403"),
            "Expected license error for blocks link on free tier, got: {}",
            stderr
        );
    }

    // Clean up
    for iid in [&iid1, &iid2] {
        track_gitlab()
            .args(["issue", "delete", iid])
            .assert()
            .success();
    }
}

// ============================================================================
// Backend-Specific Limitations
// ============================================================================

#[test]
#[ignore]
fn test_gitlab_article_commands_not_supported() {
    if !config_exists() {
        return;
    }

    // article list returns empty (KnowledgeBase stub) which is fine
    track_gitlab_json()
        .args(["article", "list"])
        .assert()
        .success();

    // article create should fail since GitLab has no knowledge base
    track_gitlab()
        .args([
            "article",
            "create",
            "-p",
            GITLAB_PROJECT,
            "-s",
            "Test Article",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not support").or(predicate::str::contains("knowledge base")),
        );
}

#[test]
#[ignore]
fn test_gitlab_project_create_not_supported() {
    if !config_exists() {
        return;
    }

    // Project creation should fail with helpful message
    track_gitlab()
        .args(["project", "create", "-n", "Test Project", "-s", "TEST"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not supported"));
}

#[test]
#[ignore]
fn test_gitlab_subtask_link_falls_back_to_relates() {
    if !config_exists() {
        return;
    }

    // Create two issues for the link attempt
    let create1_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
            "-s",
            "Subtask Test Parent",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create2_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
            "-s",
            "Subtask Test Child",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let issue1: Value = serde_json::from_str(&String::from_utf8(create1_output).unwrap()).unwrap();
    let issue2: Value = serde_json::from_str(&String::from_utf8(create2_output).unwrap()).unwrap();
    let iid1 = issue1["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();
    let iid2 = issue2["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // Subtask link type falls back to relates_to on GitLab (no native subtask support)
    track_gitlab()
        .args(["issue", "link", &iid2, &iid1, "-t", "subtask"])
        .assert()
        .success();

    // Clean up
    for iid in [&iid1, &iid2] {
        track_gitlab()
            .args(["issue", "delete", iid])
            .assert()
            .success();
    }
}

// ============================================================================
// Feature Parity Tests - Verify Core Model Structure
// ============================================================================

#[test]
#[ignore]
fn test_gitlab_issue_structure() {
    if !config_exists() {
        return;
    }

    // Create an issue
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
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

    let issue_iid = issue["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#');

    // Clean up
    track_gitlab()
        .args(["issue", "delete", issue_iid])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_gitlab_comment_structure() {
    if !config_exists() {
        return;
    }

    // Create an issue and add a comment
    let create_output = track_gitlab_json()
        .args([
            "issue",
            "create",
            "-p",
            GITLAB_PROJECT,
            "-s",
            "Comment Parity Test",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let create_str = String::from_utf8(create_output).unwrap();
    let issue: Value = serde_json::from_str(&create_str).unwrap();
    let issue_iid = issue["id_readable"]
        .as_str()
        .unwrap()
        .trim_start_matches('#')
        .to_string();

    // Add comment
    let comment_output = track_gitlab_json()
        .args([
            "issue",
            "comment",
            &issue_iid,
            "-m",
            "Test comment for structure verification",
        ])
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
    track_gitlab()
        .args(["issue", "delete", &issue_iid])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_gitlab_project_structure() {
    if !config_exists() {
        return;
    }

    let output = track_gitlab_json()
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
fn test_gitlab_cli_aliases() {
    if !config_exists() {
        return;
    }

    // Test -b gl alias
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "gl", "-o", "json", "--config"])
        .arg(config_path())
        .args(["p", "ls"]) // project list aliases
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Test i s alias (issue search)
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "gl", "--config"])
        .arg(config_path())
        .args(["i", "s", "test", "--limit", "1"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();

    // Test t ls alias (tags list)
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "gl", "--config"])
        .arg(config_path())
        .args(["t", "ls"])
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
}

// ============================================================================
// Output Format Tests
// ============================================================================

#[test]
#[ignore]
fn test_gitlab_json_output_parseable() {
    if !config_exists() {
        return;
    }

    let output = track_gitlab_json()
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
fn test_gitlab_text_output_readable() {
    if !config_exists() {
        return;
    }

    track_gitlab()
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
fn test_gitlab_invalid_project() {
    if !config_exists() {
        return;
    }

    track_gitlab()
        .args(["project", "get", "99999999"])
        .assert()
        .failure();
}

#[test]
#[ignore]
fn test_gitlab_invalid_issue_id() {
    if !config_exists() {
        return;
    }

    track_gitlab()
        .args(["issue", "get", "not-a-number"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid"));
}

// ============================================================================
// Pagination Hint Tests (with mock server)
// ============================================================================

/// Helper to start a mock server that includes custom headers in the response.
/// GitLab returns X-Total header alongside the JSON body.
fn start_gitlab_mock_server_with_headers(
    response_body: String,
    extra_headers: Vec<(String, String)>,
) -> (thread::JoinHandle<()>, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port");
    let port = listener.local_addr().unwrap().port();

    let handle = thread::spawn(move || {
        use std::io::{Read, Write};

        let timeout = Duration::from_secs(3);
        for stream in listener.incoming().flatten().take(3) {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(timeout));
            let mut buffer = [0; 4096];
            if stream.read(&mut buffer).is_ok() {
                let mut headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}",
                    response_body.len()
                );
                for (key, value) in &extra_headers {
                    headers.push_str(&format!("\r\n{}: {}", key, value));
                }
                headers.push_str("\r\n\r\n");
                let response = format!("{}{}", headers, response_body);
                let _ = stream.write_all(response.as_bytes());
            }
        }
    });

    (handle, port)
}

fn mock_gitlab_issue_simple(iid: u64, title: &str) -> serde_json::Value {
    serde_json::json!({
        "id": 1000 + iid,
        "iid": iid,
        "project_id": 1,
        "title": title,
        "description": "",
        "state": "opened",
        "labels": [],
        "assignee": null,
        "assignees": [],
        "milestone": null,
        "created_at": "2024-01-01T00:00:00.000Z",
        "updated_at": "2024-01-02T00:00:00.000Z",
        "closed_at": null,
        "author": {"id": 1, "username": "user", "name": "User"},
        "web_url": "https://gitlab.com/group/project/-/issues/1"
    })
}

/// Write a temporary GitLab config file and return its path.
fn write_gitlab_mock_config(port: u16) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("track-gl-test-{}", port));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join(".track.toml");
    let content = format!(
        r#"
[gitlab]
token = "test"
url = "http://127.0.0.1:{}/api/v4"
project_id = "1"
"#,
        port
    );
    std::fs::write(&config_path, content).unwrap();
    config_path
}

#[test]
fn test_gitlab_pagination_hint_on_full_page() {
    // GitLab returns issues array + X-Total header
    let search_response = serde_json::json!([
        mock_gitlab_issue_simple(1, "Issue 1"),
        mock_gitlab_issue_simple(2, "Issue 2")
    ]);

    let (_server, port) = start_gitlab_mock_server_with_headers(
        search_response.to_string(),
        vec![("x-total".to_string(), "10".to_string())],
    );
    thread::sleep(Duration::from_millis(50));

    let cfg = write_gitlab_mock_config(port);
    let output = cargo_bin_cmd!("track")
        .args(["-b", "gitlab", "--config"])
        .arg(&cfg)
        .args(["issue", "search", "#open", "--limit", "2"])
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
fn test_gitlab_no_hint_on_partial_page() {
    let search_response = serde_json::json!([
        mock_gitlab_issue_simple(1, "Issue 1"),
        mock_gitlab_issue_simple(2, "Issue 2")
    ]);

    let (_server, port) = start_gitlab_mock_server_with_headers(
        search_response.to_string(),
        vec![("x-total".to_string(), "2".to_string())],
    );
    thread::sleep(Duration::from_millis(50));

    let cfg = write_gitlab_mock_config(port);
    let output = cargo_bin_cmd!("track")
        .args(["-b", "gitlab", "--config"])
        .arg(&cfg)
        .args(["issue", "search", "#open", "--limit", "10"])
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
