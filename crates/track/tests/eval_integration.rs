//! Integration tests for the eval system
//!
//! These tests verify that the mock system and evaluation work correctly.
//!
//! Note: Tests that modify call logs use #[serial] to avoid race conditions.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use std::path::PathBuf;

/// Get the path to the fixtures directory (relative to workspace root)
fn fixtures_path() -> PathBuf {
    // Tests run from the workspace root when using cargo test
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent() // crates/
        .unwrap()
        .parent() // workspace root
        .unwrap()
        .join("fixtures")
        .join("scenarios")
}

/// Get path to a specific scenario
fn scenario_path(name: &str) -> PathBuf {
    fixtures_path().join(name)
}

/// Helper to run track command from workspace root
fn track() -> Command {
    let mut cmd = cargo_bin_cmd!("track");
    // Set current dir to workspace root for consistent paths
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    cmd.current_dir(workspace_root);
    cmd
}

#[test]
fn test_eval_list_shows_scenarios() {
    let path = fixtures_path();
    track()
        .args(["eval", "list", "--path", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("basic-workflow"))
        .stdout(predicate::str::contains("error-recovery"))
        .stdout(predicate::str::contains("cache-efficiency"));
}

#[test]
fn test_eval_list_json_format() {
    let path = fixtures_path();
    track()
        .args([
            "eval",
            "list",
            "--path",
            path.to_str().unwrap(),
            "-o",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("\"description\""));
}

#[test]
fn test_eval_show_displays_scenario() {
    let path = scenario_path("basic-workflow");
    track()
        .args(["eval", "show", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Scenario: basic-workflow"))
        .stdout(predicate::str::contains("Agent Prompt"))
        .stdout(predicate::str::contains("Expected Outcomes"));
}

#[test]
fn test_eval_status_when_disabled() {
    track()
        .args(["eval", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TRACK_MOCK_DIR"))
        .stdout(predicate::str::contains("disabled"));
}

#[test]
#[serial]
fn test_eval_clear_creates_empty_log() {
    let path = scenario_path("basic-workflow");
    let log_path = path.join("call_log.jsonl");

    // Write something to the log first
    fs::write(&log_path, "test content").unwrap();

    track()
        .args(["eval", "clear", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared"));

    // Verify log is empty
    let content = fs::read_to_string(&log_path).unwrap();
    assert!(content.is_empty(), "Call log should be empty after clear");
}

#[test]
#[serial]
fn test_eval_run_fails_on_empty_log() {
    let path = scenario_path("basic-workflow");
    let log_path = path.join("call_log.jsonl");

    // Ensure log is empty
    fs::write(&log_path, "").unwrap();

    track()
        .args(["eval", "run", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Call log is empty"));
}

#[test]
#[serial]
fn test_mock_mode_get_issue() {
    let path = scenario_path("basic-workflow");
    let log_path = path.join("call_log.jsonl");

    // Clear the log first
    fs::write(&log_path, "").unwrap();

    // Run a mock command
    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DEMO-1"))
        .stdout(predicate::str::contains("Implement user authentication"));

    // Verify the call was logged
    let log_content = fs::read_to_string(&log_path).unwrap();
    assert!(
        log_content.contains("get_issue"),
        "Call log should contain get_issue"
    );
}

#[test]
#[serial]
fn test_mock_mode_full_workflow() {
    let path = scenario_path("basic-workflow");
    let log_path = path.join("call_log.jsonl");

    // Clear the log
    fs::write(&log_path, "").unwrap();

    // Run the full workflow
    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .success();

    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args([
            "issue",
            "comment",
            "DEMO-1",
            "-m",
            "Starting work on this issue",
        ])
        .assert()
        .success();

    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args(["issue", "update", "DEMO-1", "--state", "In Progress"])
        .assert()
        .success();

    // Evaluate
    track()
        .args(["eval", "run", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"))
        .stdout(predicate::str::contains("issue_fetched"))
        .stdout(predicate::str::contains("comment_added"));
}

#[test]
#[serial]
fn test_eval_run_with_min_score_threshold() {
    let path = scenario_path("basic-workflow");
    let log_path = path.join("call_log.jsonl");

    // Clear and run minimal workflow
    fs::write(&log_path, "").unwrap();

    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .success();

    // This should pass with low threshold (we only did 1 command, so outcomes won't all pass)
    // But let's test that --min-score works
    track()
        .args(["eval", "run", path.to_str().unwrap(), "--min-score", "0"])
        .assert()
        .success();
}

#[test]
#[serial]
fn test_eval_run_strict_mode_fails_on_incomplete() {
    let path = scenario_path("basic-workflow");
    let log_path = path.join("call_log.jsonl");

    // Clear and run only one command (incomplete workflow)
    fs::write(&log_path, "").unwrap();

    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .success();

    // Strict mode should fail because not all outcomes are achieved
    track()
        .args(["eval", "run", path.to_str().unwrap(), "--strict"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not all expected outcomes"));
}

#[test]
#[serial]
fn test_eval_run_json_output() {
    let path = scenario_path("basic-workflow");
    let log_path = path.join("call_log.jsonl");

    // Set up a passing workflow
    fs::write(&log_path, "").unwrap();

    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .success();

    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args(["issue", "comment", "DEMO-1", "-m", "Starting work"])
        .assert()
        .success();

    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args(["issue", "update", "DEMO-1", "--state", "In Progress"])
        .assert()
        .success();

    // Check JSON output
    track()
        .args(["eval", "run", path.to_str().unwrap(), "-o", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"success\": true"))
        .stdout(predicate::str::contains("\"score_percent\""));
}

#[test]
#[serial]
fn test_mock_mode_error_handling() {
    let path = scenario_path("error-recovery");
    let log_path = path.join("call_log.jsonl");

    // Clear log
    fs::write(&log_path, "").unwrap();

    // Try to get non-existent issue (should fail with 404)
    track()
        .env("TRACK_MOCK_DIR", path.to_str().unwrap())
        .args(["issue", "get", "DEMO-999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("404").or(predicate::str::contains("not found")));

    // The call should still be logged
    let log_content = fs::read_to_string(&log_path).unwrap();
    assert!(
        log_content.contains("get_issue"),
        "Failed calls should also be logged"
    );
}
