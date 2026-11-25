//! Failures carry a stable code and, where one is known, the next step.

use predicates::prelude::*;
use std::path::Path;

mod common;
use common::{copy_scenario, temp_dir, track_in};

/// [`common::track_mock`] against a copy of the scenario `name` in `dir`,
/// with DEMO as the default project.
fn track_mock(dir: &Path, name: &str) -> assert_cmd::Command {
    let scenario = copy_scenario(dir, name);
    let mut cmd = common::track_mock(dir, &scenario);
    cmd.env("TRACKER_DEFAULT_PROJECT", "DEMO");
    cmd
}

/// Run a command that must fail with `exit_code` and return its stderr.
fn stderr_lines(cmd: &mut assert_cmd::Command, exit_code: i32) -> Vec<String> {
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(exit_code), "{output:?}");
    assert!(output.stdout.is_empty(), "errors never reach stdout");
    String::from_utf8(output.stderr)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn a_missing_issue_reports_not_found_with_a_hint() {
    let dir = temp_dir();

    let lines = stderr_lines(
        track_mock(&dir, "error-recovery").args(["issue", "get", "DEMO-999"]),
        3,
    );

    assert!(lines[0].starts_with("Error: Failed to fetch issue 'DEMO-999'"));
    assert_eq!(lines[1], "Code: not_found");
    assert!(lines[2].starts_with("Hint: "), "{lines:?}");
}

#[test]
fn an_unknown_field_reports_its_code_and_the_fix() {
    let dir = temp_dir();

    let lines = stderr_lines(
        track_mock(&dir, "cache-efficiency").args([
            "issue",
            "create",
            "-s",
            "x",
            "--field",
            "Prio=High",
        ]),
        5,
    );

    assert_eq!(
        lines[0],
        "Error: Unknown field 'Prio'. Did you mean 'Priority'? Available fields: State, Priority, Type"
    );
    assert_eq!(lines[1], "Code: unknown_field");
    assert!(lines[2].contains("track prime"));

    let lines = stderr_lines(
        track_mock(&dir, "cache-efficiency").args([
            "issue",
            "create",
            "-s",
            "x",
            "--field",
            "Priority=Urgent",
        ]),
        5,
    );
    assert_eq!(lines[1], "Code: invalid_value");
}

#[test]
fn other_cli_failures_have_codes_too() {
    let dir = temp_dir();

    let lines = stderr_lines(
        track_mock(&dir, "issue-search-templates").args(["issue", "search", "-T", "nope"]),
        3,
    );
    assert_eq!(lines[1], "Code: template_not_found");
    assert!(lines[2].contains("track workflow list"));

    let lines = stderr_lines(
        track_mock(&dir, "issue-start-complete").args(["issue", "start", "DEMO-1"]),
        5,
    );
    assert_eq!(lines[1], "Code: invalid_state");

    let lines = stderr_lines(
        track_mock(&dir, "error-recovery").args(["guide", "nope"]),
        3,
    );
    assert_eq!(lines[1], "Code: unknown_topic");

    let lines = stderr_lines(
        track_mock(&dir, "issue-get-full")
            .args(["-o", "json", "issue", "get", "DEMO-1", "--select", "nope"]),
        5,
    );
    // JSON output gets JSON errors, on one line.
    assert_eq!(lines.len(), 1, "{lines:?}");
    let error: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(error["error"]["code"], "select_no_match");

    // Asking for text errors alongside JSON output still works.
    let lines = stderr_lines(
        track_mock(&dir, "issue-get-full").args([
            "-o",
            "json",
            "--error-format",
            "text",
            "issue",
            "get",
            "DEMO-1",
            "--select",
            "nope",
        ]),
        5,
    );
    assert_eq!(lines[1], "Code: select_no_match");

    // No configuration at all.
    let output = track_in(&dir)
        .args(["issue", "search", "x"])
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("\nCode: not_configured"), "{stderr}");
    assert_eq!(output.status.code(), Some(4));
}

#[test]
fn json_error_format_is_one_object_on_stderr() {
    let dir = temp_dir();

    for from_env in [false, true] {
        let mut cmd = track_mock(&dir, "cache-efficiency");
        if from_env {
            cmd.env("TRACK_ERROR_FORMAT", "json");
        } else {
            cmd.args(["--error-format", "json"]);
        }
        let lines = stderr_lines(
            cmd.args(["issue", "create", "-s", "x", "--field", "Prio=High"]),
            5,
        );

        assert_eq!(lines.len(), 1, "{lines:?}");
        let json: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(json["error"]["code"], "unknown_field");
        assert_eq!(json["error"]["retryable"], false);
        assert!(json["error"]["message"].as_str().unwrap().contains("Prio"));
        assert!(json["error"]["hint"].is_string());
    }
}

#[test]
fn batch_failures_carry_a_code_per_issue() {
    let dir = temp_dir();

    let output = track_mock(&dir, "error-recovery")
        .args(["-o", "json", "issue", "delete", "DEMO-1,DEMO-999"])
        .output()
        .unwrap();

    // Some issues failed, some did not: its own exit code.
    assert_eq!(output.status.code(), Some(6));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let failed: Vec<&serde_json::Value> = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|result| result["success"] == false)
        .collect();
    assert!(!failed.is_empty(), "{json}");
    for result in failed {
        assert!(result["code"].is_string(), "{result}");
    }
}

#[test]
fn success_prints_no_code_or_hint() {
    let dir = temp_dir();

    track_mock(&dir, "error-recovery")
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Code:").not());
}
