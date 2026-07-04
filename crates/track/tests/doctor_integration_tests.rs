//! Integration tests for `track doctor` (issue #293).
//!
//! All tests run offline: remote-capable paths go through the tracker-mock
//! client (TRACK_MOCK_DIR), and failure paths use configs that are rejected
//! before any client is built.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

/// Create a unique temp directory for test isolation.
fn temp_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "track-doctor-test-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Build a track command running in the given directory with a clean env.
fn track_in(dir: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env_remove("TRACKER_URL")
        .env_remove("TRACKER_TOKEN")
        .env_remove("TRACKER_BACKEND")
        .env_remove("TRACKER_CONFIG")
        .env_remove("YOUTRACK_URL")
        .env_remove("YOUTRACK_TOKEN")
        .env_remove("JIRA_URL")
        .env_remove("JIRA_EMAIL")
        .env_remove("JIRA_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GITHUB_OWNER")
        .env_remove("GITHUB_REPO")
        .env_remove("GITHUB_API_URL")
        .env_remove("GITLAB_TOKEN")
        .env_remove("GITLAB_URL")
        .env_remove("GITLAB_PROJECT_ID")
        .env_remove("GITLAB_NAMESPACE")
        .env_remove("LINEAR_TOKEN")
        .env_remove("LINEAR_API_URL")
        .env_remove("LINEAR_URL")
        .env_remove("LINEAR_DEFAULT_TEAM")
        .env_remove("LINEAR_DEFAULT_PROJECT")
        .env_remove("TRACK_MOCK_DIR");
    cmd
}

fn fixtures_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("scenarios")
}

fn copy_dir_recursive(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(&source, &target);
        } else {
            fs::copy(&source, &target).unwrap();
        }
    }
}

fn copy_scenario(dir: &Path, name: &str) -> PathBuf {
    let scenario = dir.join(name);
    copy_dir_recursive(&fixtures_path().join(name), &scenario);
    fs::write(scenario.join("call_log.jsonl"), "").unwrap();
    scenario
}

fn doctor_json(dir: &Path, scenario: &Path, extra_args: &[&str]) -> serde_json::Value {
    let mut cmd = track_in(dir);
    cmd.env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args(["-o", "json", "doctor"])
        .args(extra_args);
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "doctor failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn check<'a>(
    report: &'a serde_json::Value,
    backend_idx: usize,
    name: &str,
) -> &'a serde_json::Value {
    report["backends"][backend_idx]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("check '{}' missing from report: {}", name, report))
}

#[test]
fn doctor_json_reports_per_check_statuses_against_mock() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let report = doctor_json(&dir, &scenario, &[]);

    assert_eq!(report["summary"]["backends_checked"], 1);
    assert_eq!(report["backends"][0]["backend"], "youtrack");

    assert_eq!(check(&report, 0, "config_valid")["status"], "ok");
    assert_eq!(check(&report, 0, "auth_connectivity")["status"], "ok");
    assert_eq!(check(&report, 0, "project_resolution")["status"], "ok");
    assert_eq!(check(&report, 0, "issue_search")["status"], "ok");
    assert!(
        check(&report, 0, "issue_search")["sample_count"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert_eq!(check(&report, 0, "issue_read")["status"], "ok");
    assert_eq!(check(&report, 0, "comments_read")["status"], "ok");
    assert_eq!(check(&report, 0, "links_read")["status"], "ok");
    assert_eq!(check(&report, 0, "field_schema")["status"], "ok");

    // Field admin is unsupported by the mock (default trait impl) -> skipped.
    assert_eq!(check(&report, 0, "field_admin")["status"], "skipped");

    // The basic-workflow scenario has no articles fixture: the mock answers
    // 404, which the doctor classifies as degraded (reachable, missing).
    assert_eq!(check(&report, 0, "articles")["status"], "degraded");

    // Mutating checks stay skipped without --write-check.
    assert_eq!(check(&report, 0, "write_validation")["status"], "skipped");

    // Degraded articles check pulls the backend to degraded, with a
    // reads-are-usable recommendation.
    assert_eq!(report["backends"][0]["status"], "degraded");
    assert!(
        report["backends"][0]["recommendation"]
            .as_str()
            .unwrap()
            .contains("Read/search workflows are usable")
    );
    assert_eq!(report["summary"]["degraded"], 1);
    assert_eq!(report["summary"]["failed"], 0);

    // Secrets must never appear in the report.
    assert!(!report.to_string().contains("mock-token"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn doctor_write_check_validates_locally_without_mutations() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let report = doctor_json(&dir, &scenario, &["--write-check"]);
    let wv = check(&report, 0, "write_validation");
    assert_eq!(wv["status"], "ok");
    assert!(
        wv["message"].as_str().unwrap().contains("no remote writes"),
        "write_validation must document that it does not mutate: {wv}"
    );

    // Assert no mutating methods were called on the mock.
    let log = fs::read_to_string(scenario.join("call_log.jsonl")).unwrap();
    for mutating in [
        "create_issue",
        "update_issue",
        "delete_issue",
        "add_comment",
        "link_issues",
        "create_article",
    ] {
        assert!(
            !log.contains(&format!("\"method\":\"{}\"", mutating)),
            "doctor must not call {}: {}",
            mutating,
            log
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn doctor_respects_project_flag() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    let report = doctor_json(&dir, &scenario, &["--project", "DEMO"]);
    let resolution = check(&report, 0, "project_resolution");
    assert_eq!(resolution["status"], "ok");
    assert!(
        resolution["message"]
            .as_str()
            .unwrap()
            .contains("resolved 'DEMO'")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn doctor_all_backends_enumerates_configured_backends() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    fs::write(
        dir.join(".track.toml"),
        r#"
backend = "youtrack"

[youtrack]
url = "https://yt.mock.test"
token = "yt-token"

[gitlab]
url = "https://gitlab.mock.test/api/v4"
token = "gl-token"
project_id = "123"
"#,
    )
    .unwrap();

    let output = track_in(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["-o", "json", "doctor", "--all-backends"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(report["summary"]["backends_checked"], 2);
    let backends: Vec<&str> = report["backends"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["backend"].as_str().unwrap())
        .collect();
    assert_eq!(backends, vec!["youtrack", "gitlab"]);

    // Config source should identify the file, never the tokens.
    let text = report.to_string();
    assert!(text.contains(".track.toml"));
    assert!(!text.contains("yt-token"));
    assert!(!text.contains("gl-token"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn doctor_invalid_config_fails_only_under_strict() {
    // No config at all: config_valid fails, remote checks are skipped, and no
    // network is touched. Non-strict still exits 0; strict exits non-zero.
    let dir = temp_dir();

    let output = track_in(&dir)
        .args(["-o", "json", "doctor"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "doctor without --strict must exit 0 even when checks fail"
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["failed"], 1);
    assert_eq!(report["backends"][0]["status"], "failed");

    let checks = report["backends"][0]["checks"].as_array().unwrap();
    let config_valid = checks.iter().find(|c| c["name"] == "config_valid").unwrap();
    assert_eq!(config_valid["status"], "failed");
    // All remote checks skipped when no client can be built.
    for c in checks.iter().filter(|c| c["name"] != "config_valid") {
        assert_eq!(c["status"], "skipped", "check {} should be skipped", c);
    }

    track_in(&dir)
        .args(["doctor", "--strict"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--strict"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn doctor_strict_passes_when_only_degraded() {
    // basic-workflow yields one degraded check (articles); degraded must not
    // trip --strict.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    track_in(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args(["doctor", "--strict"])
        .assert()
        .success();

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn doctor_text_output_lists_checks() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    track_in(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("youtrack"))
        .stdout(predicate::str::contains("auth_connectivity"))
        .stdout(predicate::str::contains("issue_search"))
        .stdout(predicate::str::contains("write_validation"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn doctor_help_documents_flags() {
    let dir = temp_dir();
    track_in(&dir)
        .args(["doctor", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--all-backends"))
        .stdout(predicate::str::contains("--write-check"))
        .stdout(predicate::str::contains("--strict"));

    let _ = fs::remove_dir_all(&dir);
}
