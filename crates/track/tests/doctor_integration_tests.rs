//! Integration tests for `track doctor` (issue #293).
//!
//! All tests run offline: remote-capable paths go through the tracker-mock
//! client (TRACK_MOCK_DIR), and failure paths use configs that are rejected
//! before any client is built.

use predicates::prelude::*;
use std::fs;
use std::path::Path;

mod common;
use common::{copy_scenario, temp_dir, track_in, track_mock};

fn doctor_json(dir: &Path, scenario: &Path, extra_args: &[&str]) -> serde_json::Value {
    let mut cmd = track_mock(dir, scenario);
    cmd.args(["-o", "json", "doctor"]).args(extra_args);
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
}

#[test]
fn doctor_all_backends_scopes_cli_overrides_to_effective_backend() {
    // Global --url/--token belong to the effective backend only; other
    // audited backends must keep their own configured URL.
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
        .args([
            "--url",
            "https://override.test",
            "--token",
            "override-token",
        ])
        .args(["-o", "json", "doctor", "--all-backends"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(report["backends"][0]["backend"], "youtrack");
    assert_eq!(
        report["backends"][0]["config"]["url"],
        "https://override.test"
    );
    assert_eq!(report["backends"][1]["backend"], "gitlab");
    assert_eq!(
        report["backends"][1]["config"]["url"],
        "https://gitlab.mock.test/api/v4"
    );
}

#[test]
fn doctor_all_backends_includes_explicit_backend_flag() {
    // An explicit -b backend must be audited under --all-backends even when
    // the config only names other backends.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    fs::write(
        dir.join(".track.toml"),
        r#"
[youtrack]
url = "https://yt.mock.test"
token = "yt-token"
"#,
    )
    .unwrap();

    let output = track_in(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["-b", "gitlab"])
        .args([
            "--url",
            "https://gitlab.mock.test/api/v4",
            "--token",
            "gl-token",
        ])
        .args(["-o", "json", "doctor", "--all-backends"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let backends: Vec<&str> = report["backends"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["backend"].as_str().unwrap())
        .collect();
    assert_eq!(backends, vec!["youtrack", "gitlab"]);
}

#[test]
fn doctor_uses_backend_named_in_explicit_config_file() {
    // `--config` must also decide the backend, not only the settings: a
    // GitLab config passed explicitly is audited as GitLab, not as YouTrack.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    let config_path = dir.join("gitlab.toml");
    fs::write(
        &config_path,
        r#"
backend = "gitlab"

[gitlab]
url = "https://gitlab.mock.test/api/v4"
token = "gl-token"
project_id = "123"
"#,
    )
    .unwrap();

    let output = track_in(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .arg("--config")
        .arg(&config_path)
        .args(["-o", "json", "doctor"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(report["backends"][0]["backend"], "gitlab");
    assert_eq!(check(&report, 0, "config_valid")["status"], "ok");
    assert_eq!(
        report["backends"][0]["config"]["url"],
        "https://gitlab.mock.test/api/v4"
    );
}

#[test]
fn doctor_accepts_numeric_values_from_env() {
    // GitLab project ids are numeric; GITLAB_PROJECT_ID=123 must configure
    // the string setting instead of failing to load the config.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    fs::write(
        dir.join(".track.toml"),
        r#"
backend = "gitlab"

[gitlab]
url = "https://gitlab.mock.test/api/v4"
token = "gl-token"
"#,
    )
    .unwrap();

    let output = track_in(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .env("GITLAB_PROJECT_ID", "123")
        .args(["-o", "json", "doctor"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(report["backends"][0]["backend"], "gitlab");
    assert_eq!(
        check(&report, 0, "config_valid")["status"],
        "ok",
        "report: {report}"
    );
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
}

#[test]
fn doctor_strict_passes_when_only_degraded() {
    // basic-workflow yields one degraded check (articles); degraded must not
    // trip --strict.
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    track_mock(&dir, &scenario)
        .args(["doctor", "--strict"])
        .assert()
        .success();
}

#[test]
fn doctor_text_output_lists_checks() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");

    track_mock(&dir, &scenario)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("youtrack"))
        .stdout(predicate::str::contains("auth_connectivity"))
        .stdout(predicate::str::contains("issue_search"))
        .stdout(predicate::str::contains("write_validation"));
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
}
