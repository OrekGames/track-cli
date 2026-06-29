use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

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
        "track-apply-test-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn track_in(dir: &Path, scenario: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env("TRACK_MOCK_DIR", scenario)
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
        .args(["--url", "https://mock.test", "--token", "mock-token"]);
    cmd
}

fn write_scenario(dir: &Path, manifest: &str, responses: &[(&str, Value)]) -> PathBuf {
    let scenario = dir.join("scenario");
    let responses_dir = scenario.join("responses");
    fs::create_dir_all(&responses_dir).unwrap();
    fs::write(scenario.join("manifest.toml"), manifest).unwrap();
    fs::write(scenario.join("call_log.jsonl"), "").unwrap();
    for (name, value) in responses {
        fs::write(
            responses_dir.join(name),
            serde_json::to_string(value).unwrap(),
        )
        .unwrap();
    }
    scenario
}

fn base_manifest(extra: &str) -> String {
    format!(
        r#"
[[responses]]
method = "resolve_project_id"
file = "resolve_project_id_DEMO.json"
[responses.args]
identifier = "DEMO"

[[responses]]
method = "get_project_custom_fields"
file = "project_custom_fields.json"
[responses.args]
project_id = "*"

{extra}
"#
    )
}

fn project_fields() -> Value {
    json!([
        {
            "id": "field-state",
            "name": "State",
            "field_type": "state[1]",
            "required": false,
            "values": ["Open", "In Progress", "Done"]
        },
        {
            "id": "field-priority",
            "name": "Priority",
            "field_type": "enum[1]",
            "required": false,
            "values": ["Major", "Minor", "Normal"]
        },
        {
            "id": "field-platform",
            "name": "Platform",
            "field_type": "enum[*]",
            "required": false,
            "values": ["macOS", "Linux"]
        }
    ])
}

fn issue_json(id: &str, summary: &str, state: &str, priority: &str) -> Value {
    json!({
        "id": format!("internal-{id}"),
        "id_readable": id,
        "summary": summary,
        "description": "Issue description",
        "project": {
            "id": "0-1",
            "name": "Demo Project",
            "short_name": "DEMO"
        },
        "custom_fields": [
            {
                "State": {
                    "name": "State",
                    "value": state,
                    "is_resolved": state == "Done"
                }
            },
            {
                "SingleEnum": {
                    "name": "Priority",
                    "value": priority
                }
            }
        ],
        "tags": [],
        "created": "2024-01-10T09:00:00Z",
        "updated": "2024-01-15T14:30:00Z",
        "resolved": null
    })
}

fn comment_json(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "text": text,
        "author": null,
        "created": null
    })
}

fn standard_responses() -> Vec<(&'static str, Value)> {
    vec![
        ("resolve_project_id_DEMO.json", json!("0-1")),
        ("project_custom_fields.json", project_fields()),
    ]
}

fn write_plan(dir: &Path, name: &str, plan: Value) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, serde_json::to_string_pretty(&plan).unwrap()).unwrap();
    path
}

fn parse_stdout_json(output: &[u8]) -> Value {
    serde_json::from_slice(output).unwrap()
}

fn mock_call_methods(scenario: &Path) -> Vec<String> {
    let log = fs::read_to_string(scenario.join("call_log.jsonl")).unwrap_or_default();
    log.lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .map(|entry| entry["method"].as_str().unwrap().to_string())
        .collect()
}

fn method_count(methods: &[String], method: &str) -> usize {
    methods
        .iter()
        .filter(|actual| actual.as_str() == method)
        .count()
}

#[test]
fn apply_runs_create_update_comment_and_link_plan() {
    let dir = temp_dir();
    let manifest = base_manifest(
        r#"
[[responses]]
method = "create_issue"
file = "create_parent.json"
[responses.args]
project = "0-1"
summary = "Parent issue"

[[responses]]
method = "create_issue"
file = "create_child.json"
[responses.args]
project = "0-1"
summary = "Child issue"

[[responses]]
method = "get_issue"
file = "get_parent.json"
[responses.args]
id = "DEMO-100"

[[responses]]
method = "update_issue"
file = "update_parent.json"
[responses.args]
id = "DEMO-100"

[[responses]]
method = "add_comment"
file = "comment_parent.json"
[responses.args]
issue_id = "DEMO-100"

[[responses]]
method = "link_subtask"
file = "ok.json"
[responses.args]
child = "DEMO-101"
parent = "DEMO-100"
"#,
    );
    let mut responses = standard_responses();
    responses.extend([
        (
            "create_parent.json",
            issue_json("DEMO-100", "Parent issue", "Open", "Major"),
        ),
        (
            "create_child.json",
            issue_json("DEMO-101", "Child issue", "Open", "Normal"),
        ),
        (
            "get_parent.json",
            issue_json("DEMO-100", "Parent issue", "Open", "Major"),
        ),
        (
            "update_parent.json",
            issue_json("DEMO-100", "Parent issue", "In Progress", "Major"),
        ),
        (
            "comment_parent.json",
            comment_json("comment-1", "Created from apply"),
        ),
        ("ok.json", Value::Null),
    ]);
    let scenario = write_scenario(&dir, &manifest, &responses);
    let plan = write_plan(
        &dir,
        "plan.json",
        json!({
            "version": 1,
            "defaults": {"project": "DEMO", "validate": true},
            "operations": [
                {
                    "ref": "parent",
                    "op": "create_issue",
                    "summary": "Parent issue",
                    "fields": {"Priority": "Major"}
                },
                {
                    "ref": "child",
                    "op": "create_issue",
                    "summary": "Child issue",
                    "parent": "$parent"
                },
                {
                    "op": "update_issue",
                    "issue": "$parent",
                    "fields": {"State": "In Progress"}
                },
                {
                    "op": "comment",
                    "issue": "$parent",
                    "body": "Created from apply"
                },
                {
                    "op": "link",
                    "source": "$child",
                    "target": "$parent",
                    "type": "subtask"
                }
            ]
        }),
    );

    let output = track_in(&dir, &scenario)
        .args(["-o", "json", "apply"])
        .arg(&plan)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = parse_stdout_json(&output);

    assert_eq!(json["success"], true);
    assert_eq!(json["refs"]["parent"], "DEMO-100");
    assert_eq!(json["refs"]["child"], "DEMO-101");
    assert_eq!(json["summary"]["total"], 5);
    assert_eq!(json["results"][0]["status"], "created");
    assert_eq!(json["results"][4]["status"], "linked");

    let methods = mock_call_methods(&scenario);
    assert_eq!(method_count(&methods, "create_issue"), 2);
    assert_eq!(method_count(&methods, "update_issue"), 1);
    assert_eq!(method_count(&methods, "add_comment"), 1);
    assert_eq!(method_count(&methods, "link_subtask"), 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_dry_run_does_not_call_mutating_methods() {
    let dir = temp_dir();
    let scenario = write_scenario(&dir, &base_manifest(""), &standard_responses());
    let plan = write_plan(
        &dir,
        "plan.json",
        json!({
            "version": 1,
            "defaults": {"project": "DEMO", "validate": true},
            "operations": [
                {
                    "ref": "parent",
                    "op": "create_issue",
                    "summary": "Parent issue",
                    "fields": {"Priority": "Major"}
                }
            ]
        }),
    );

    let output = track_in(&dir, &scenario)
        .args(["-o", "json", "apply"])
        .arg(&plan)
        .arg("--dry-run")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = parse_stdout_json(&output);

    assert_eq!(json["success"], true);
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["results"][0]["status"], "dry_run");

    let methods = mock_call_methods(&scenario);
    assert_eq!(method_count(&methods, "create_issue"), 0);
    assert_eq!(method_count(&methods, "update_issue"), 0);
    assert_eq!(method_count(&methods, "add_comment"), 0);
    assert_eq!(method_count(&methods, "link_subtask"), 0);
    assert_eq!(method_count(&methods, "delete_issue"), 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_reads_plan_from_stdin() {
    let dir = temp_dir();
    let scenario = write_scenario(&dir, &base_manifest(""), &standard_responses());
    let plan = json!({
        "version": 1,
        "defaults": {"project": "DEMO"},
        "operations": [
            {
                "ref": "parent",
                "op": "create_issue",
                "summary": "Parent from stdin"
            }
        ]
    });

    let output = track_in(&dir, &scenario)
        .args(["-o", "json", "apply", "-", "--dry-run"])
        .write_stdin(serde_json::to_string(&plan).unwrap())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = parse_stdout_json(&output);

    assert_eq!(json["success"], true);
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["refs"]["parent"], "planned:parent");

    let methods = mock_call_methods(&scenario);
    assert_eq!(method_count(&methods, "create_issue"), 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_validation_failure_stops_before_mutation() {
    let dir = temp_dir();
    let scenario = write_scenario(&dir, &base_manifest(""), &standard_responses());
    let plan = write_plan(
        &dir,
        "plan.json",
        json!({
            "version": 1,
            "defaults": {"project": "DEMO", "validate": true},
            "operations": [
                {
                    "op": "create_issue",
                    "summary": "Invalid priority",
                    "fields": {"Priority": "Impossible"}
                }
            ]
        }),
    );

    let output = track_in(&dir, &scenario)
        .args(["-o", "json", "apply"])
        .arg(&plan)
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let json = parse_stdout_json(&output);

    assert_eq!(json["success"], false);
    assert_eq!(json["results"][0]["status"], "failed");
    assert!(
        json["results"][0]["error"]
            .as_str()
            .unwrap()
            .contains("Invalid value")
    );

    let methods = mock_call_methods(&scenario);
    assert_eq!(method_count(&methods, "create_issue"), 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_dedupe_reuse_skips_create_and_populates_refs() {
    let dir = temp_dir();
    let manifest = base_manifest(
        r#"
[[responses]]
method = "search_issues"
file = "search_existing.json"
[responses.args]
query = "project: DEMO summary: {Existing}"
limit = "2"
skip = "0"

[[responses]]
method = "add_comment"
file = "comment_existing.json"
[responses.args]
issue_id = "DEMO-9"
"#,
    );
    let mut responses = standard_responses();
    responses.extend([
        (
            "search_existing.json",
            json!([issue_json("DEMO-9", "Existing", "Open", "Normal")]),
        ),
        (
            "comment_existing.json",
            comment_json("comment-9", "Reused issue"),
        ),
    ]);
    let scenario = write_scenario(&dir, &manifest, &responses);
    let plan = write_plan(
        &dir,
        "plan.json",
        json!({
            "version": 1,
            "defaults": {"project": "DEMO"},
            "operations": [
                {
                    "ref": "existing",
                    "op": "create_issue",
                    "summary": "Existing",
                    "dedupe": {
                        "query": "project: DEMO summary: {Existing}",
                        "on_match": "reuse"
                    }
                },
                {
                    "op": "comment",
                    "issue": "$existing",
                    "body": "Reused issue"
                }
            ]
        }),
    );

    let output = track_in(&dir, &scenario)
        .args(["-o", "json", "apply"])
        .arg(&plan)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = parse_stdout_json(&output);

    assert_eq!(json["success"], true);
    assert_eq!(json["refs"]["existing"], "DEMO-9");
    assert_eq!(json["results"][0]["status"], "reused");

    let methods = mock_call_methods(&scenario);
    assert_eq!(method_count(&methods, "create_issue"), 0);
    assert_eq!(method_count(&methods, "add_comment"), 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_multiple_dedupe_matches_fails_without_mutation() {
    let dir = temp_dir();
    let manifest = base_manifest(
        r#"
[[responses]]
method = "search_issues"
file = "search_multiple.json"
[responses.args]
query = "project: DEMO summary: {Duplicate}"
limit = "2"
skip = "0"
"#,
    );
    let mut responses = standard_responses();
    responses.push((
        "search_multiple.json",
        json!([
            issue_json("DEMO-9", "Duplicate", "Open", "Normal"),
            issue_json("DEMO-10", "Duplicate", "Open", "Normal")
        ]),
    ));
    let scenario = write_scenario(&dir, &manifest, &responses);
    let plan = write_plan(
        &dir,
        "plan.json",
        json!({
            "version": 1,
            "defaults": {"project": "DEMO"},
            "operations": [
                {
                    "ref": "duplicate",
                    "op": "create_issue",
                    "summary": "Duplicate",
                    "dedupe": {
                        "query": "project: DEMO summary: {Duplicate}",
                        "on_match": "reuse"
                    }
                }
            ]
        }),
    );

    let output = track_in(&dir, &scenario)
        .args(["-o", "json", "apply"])
        .arg(&plan)
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let json = parse_stdout_json(&output);

    assert_eq!(json["success"], false);
    assert!(
        json["results"][0]["error"]
            .as_str()
            .unwrap()
            .contains("multiple issues")
    );

    let methods = mock_call_methods(&scenario);
    assert_eq!(method_count(&methods, "create_issue"), 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_resume_skips_completed_operations() {
    let dir = temp_dir();
    let failing_manifest = base_manifest(
        r#"
[[responses]]
method = "create_issue"
file = "create_resume.json"
[responses.args]
project = "0-1"
summary = "Resume parent"

[[responses]]
method = "update_issue"
file = "update_error.json"
status = 500
[responses.args]
id = "DEMO-200"
"#,
    );
    let mut responses = standard_responses();
    responses.extend([
        (
            "create_resume.json",
            issue_json("DEMO-200", "Resume parent", "Open", "Normal"),
        ),
        ("update_error.json", json!({"message": "temporary failure"})),
    ]);
    let scenario = write_scenario(&dir, &failing_manifest, &responses);
    let plan = write_plan(
        &dir,
        "plan.json",
        json!({
            "version": 1,
            "defaults": {"project": "DEMO"},
            "operations": [
                {
                    "ref": "parent",
                    "op": "create_issue",
                    "summary": "Resume parent"
                },
                {
                    "op": "update_issue",
                    "issue": "$parent",
                    "summary": "Resume parent updated"
                }
            ]
        }),
    );
    let resume = dir.join("state.json");

    track_in(&dir, &scenario)
        .args(["-o", "json", "apply"])
        .arg(&plan)
        .arg("--resume")
        .arg(&resume)
        .assert()
        .failure();

    let state: Value = serde_json::from_str(&fs::read_to_string(&resume).unwrap()).unwrap();
    assert_eq!(state["completed"][0], 0);
    assert_eq!(state["refs"]["parent"], "DEMO-200");

    let passing_manifest = base_manifest(
        r#"
[[responses]]
method = "create_issue"
file = "create_resume.json"
[responses.args]
project = "0-1"
summary = "Resume parent"

[[responses]]
method = "update_issue"
file = "update_resume.json"
[responses.args]
id = "DEMO-200"
"#,
    );
    fs::write(scenario.join("manifest.toml"), passing_manifest).unwrap();
    fs::write(
        scenario.join("responses/update_resume.json"),
        serde_json::to_string(&issue_json(
            "DEMO-200",
            "Resume parent updated",
            "Open",
            "Normal",
        ))
        .unwrap(),
    )
    .unwrap();

    let output = track_in(&dir, &scenario)
        .args(["-o", "json", "apply"])
        .arg(&plan)
        .arg("--resume")
        .arg(&resume)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = parse_stdout_json(&output);

    assert_eq!(json["success"], true);
    assert_eq!(json["resumed"], true);
    assert_eq!(json["results"][0]["status"], "skipped");
    assert_eq!(json["results"][1]["status"], "updated");

    let methods = mock_call_methods(&scenario);
    assert_eq!(method_count(&methods, "create_issue"), 1);
    assert_eq!(method_count(&methods, "update_issue"), 2);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_delete_requires_allow_delete_for_real_execution() {
    let dir = temp_dir();
    let manifest = base_manifest(
        r#"
[[responses]]
method = "delete_issue"
file = "ok.json"
[responses.args]
id = "DEMO-1"
"#,
    );
    let mut responses = standard_responses();
    responses.push(("ok.json", Value::Null));
    let scenario = write_scenario(&dir, &manifest, &responses);
    let plan = write_plan(
        &dir,
        "plan.json",
        json!({
            "version": 1,
            "operations": [
                {"op": "delete_issue", "issue": "DEMO-1"}
            ]
        }),
    );

    let output = track_in(&dir, &scenario)
        .args(["-o", "json", "apply"])
        .arg(&plan)
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let json = parse_stdout_json(&output);
    assert_eq!(json["success"], false);
    assert!(
        json["results"][0]["error"]
            .as_str()
            .unwrap()
            .contains("--allow-delete")
    );
    assert_eq!(
        method_count(&mock_call_methods(&scenario), "delete_issue"),
        0
    );

    let output = track_in(&dir, &scenario)
        .args(["-o", "json", "apply"])
        .arg(&plan)
        .arg("--allow-delete")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = parse_stdout_json(&output);
    assert_eq!(json["success"], true);
    assert_eq!(json["results"][0]["status"], "deleted");
    assert_eq!(
        method_count(&mock_call_methods(&scenario), "delete_issue"),
        1
    );

    let _ = fs::remove_dir_all(&dir);
}
