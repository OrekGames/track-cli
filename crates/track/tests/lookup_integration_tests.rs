//! Lookups that read through the cache, validation that is on by default,
//! and `start`/`complete` checking the state before they write.
//!
//! Every test counts backend calls through the mock's call log: the point of
//! the cache is fewer of them, and the point of the safeguards is that a
//! cached "no" is checked with the backend before it becomes an error.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
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
        "track-lookup-test-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

/// A private copy of a scenario, so its call log and responses can change.
fn copy_scenario(dir: &Path, name: &str) -> PathBuf {
    let scenario = dir.join(name);
    copy_dir_recursive(&fixtures_path().join(name), &scenario);
    fs::write(scenario.join("call_log.jsonl"), "").unwrap();
    scenario
}

/// `track` against a mock scenario, with HOME (and so the cache) in `dir`.
fn track_mock(dir: &Path, scenario: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .env("TRACKER_DEFAULT_PROJECT", "DEMO")
        .env_remove("TRACK_CACHE_DIR")
        .env_remove("TRACK_NO_CACHE")
        .env_remove("TRACK_SCHEMA_TTL")
        .args(["--url", "https://mock.test", "--token", "mock-token"]);
    cmd
}

/// Backend methods called since the log was last cleared, then clear it.
fn take_calls(scenario: &Path) -> Vec<String> {
    let path = scenario.join("call_log.jsonl");
    let log = fs::read_to_string(&path).unwrap_or_default();
    fs::write(&path, "").unwrap();
    log.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .map(|entry| entry["method"].as_str().unwrap().to_string())
        .filter(|method| !method.starts_with("cache_"))
        .collect()
}

/// Populate the cache for `scenario` and forget the calls that took.
fn warm_cache(dir: &Path, scenario: &Path) {
    track_mock(dir, scenario)
        .args(["cache", "refresh"])
        .assert()
        .success();
    take_calls(scenario);
}

const CREATE: [&str; 9] = [
    "issue",
    "create",
    "-p",
    "DEMO",
    "-s",
    "Fix login timeout bug",
    "--field",
    "Priority=Normal",
    "--field",
];

fn create_args(last_field: &str) -> Vec<&str> {
    let mut args = CREATE.to_vec();
    args.push(last_field);
    args
}

// =============================================================================
// Read-through lookups
// =============================================================================

#[test]
fn cold_create_looks_up_what_it_needs_and_no_more() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");

    track_mock(&dir, &scenario)
        .args(create_args("Type=Bug"))
        .assert()
        .success();

    // Resolve the project (the mock lists projects to do it), read its
    // schema once for typing and validation, create.
    assert_eq!(
        take_calls(&scenario),
        ["list_projects", "get_project_custom_fields", "create_issue"]
    );

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn warm_create_makes_only_the_write() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);

    track_mock(&dir, &scenario)
        .args(create_args("Type=Bug"))
        .assert()
        .success();

    assert_eq!(take_calls(&scenario), ["create_issue"]);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn no_cache_flag_and_env_look_everything_up_again() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    let live = ["list_projects", "get_project_custom_fields", "create_issue"];

    track_mock(&dir, &scenario)
        .arg("--no-cache")
        .args(create_args("Type=Bug"))
        .assert()
        .success();
    assert_eq!(take_calls(&scenario), live);

    track_mock(&dir, &scenario)
        .env("TRACK_NO_CACHE", "1")
        .args(create_args("Type=Bug"))
        .assert()
        .success();
    assert_eq!(take_calls(&scenario), live);

    fs::remove_dir_all(&dir).unwrap();
}

/// Rewrite every `fetched_at` in a cache file to `timestamp`.
fn backdate(path: &Path, timestamp: &str) {
    fn walk(value: &mut serde_json::Value, timestamp: &str) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    if key == "fetched_at" {
                        *child = serde_json::Value::String(timestamp.to_string());
                    } else {
                        walk(child, timestamp);
                    }
                }
            }
            serde_json::Value::Array(items) => {
                items.iter_mut().for_each(|item| walk(item, timestamp))
            }
            _ => {}
        }
    }
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    walk(&mut value, timestamp);
    fs::write(path, serde_json::to_string(&value).unwrap()).unwrap();
}

#[test]
fn an_expired_schema_is_fetched_again() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    // Lookups trust cached answers for a day.
    let cache = dir.join(".tracker-cli/cache/youtrack");
    backdate(
        &cache.join("projects/DEMO/schema.json"),
        "2020-01-01T00:00:00Z",
    );
    backdate(
        &cache.join("backend/resolved_projects.json"),
        "2020-01-01T00:00:00Z",
    );

    track_mock(&dir, &scenario)
        .args(create_args("Type=Bug"))
        .assert()
        .success();
    assert_eq!(
        take_calls(&scenario),
        ["list_projects", "get_project_custom_fields", "create_issue"]
    );

    // A longer lifetime can be asked for.
    backdate(
        &cache.join("projects/DEMO/schema.json"),
        "2020-01-01T00:00:00Z",
    );
    backdate(
        &cache.join("backend/resolved_projects.json"),
        "2020-01-01T00:00:00Z",
    );
    track_mock(&dir, &scenario)
        .env("TRACK_SCHEMA_TTL", "9999d")
        .args(create_args("Type=Bug"))
        .assert()
        .success();
    assert_eq!(take_calls(&scenario), ["create_issue"]);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_cache_from_another_instance_is_not_used() {
    let dir = temp_dir();
    let first = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &first);
    // The same fixture under another path stands for another tracker.
    let second = dir.join("other-instance");
    copy_dir_recursive(&first, &second);
    fs::write(second.join("call_log.jsonl"), "").unwrap();

    track_mock(&dir, &second)
        .args(create_args("Type=Bug"))
        .assert()
        .success();

    assert_eq!(
        take_calls(&second),
        ["list_projects", "get_project_custom_fields", "create_issue"]
    );

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn reading_fields_refreshes_the_cached_schema() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    let schema = dir.join(".tracker-cli/cache/youtrack/projects/DEMO/schema.json");
    let before = fs::read_to_string(&schema).unwrap();

    // The tracker gains a priority after the cache was written.
    let fields = scenario.join("responses/project_custom_fields.json");
    let updated = fs::read_to_string(&fields)
        .unwrap()
        .replace("\"Critical\"", "\"Critical\", \"Blocker\"");
    fs::write(&fields, updated).unwrap();

    track_mock(&dir, &scenario)
        .args(["project", "fields", "DEMO"])
        .assert()
        .success();

    let after = fs::read_to_string(&schema).unwrap();
    assert_ne!(before, after);
    assert!(after.contains("Blocker"));

    fs::remove_dir_all(&dir).unwrap();
}

// =============================================================================
// Validation on by default
// =============================================================================

#[test]
fn an_invalid_value_is_refused_before_the_write() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);

    track_mock(&dir, &scenario)
        .args(create_args("Type=Bugg"))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Invalid value 'Bugg' for field 'Type'. Did you mean 'Bug'? \
             Valid values: Bug, Task, Feature, Improvement",
        ));

    // The cached schema said no, so the backend was asked once; nothing
    // was created.
    assert_eq!(take_calls(&scenario), ["get_project_custom_fields"]);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_unknown_field_names_the_likely_one() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");

    track_mock(&dir, &scenario)
        .args([
            "issue", "create", "-p", "DEMO", "-s", "x", "--field", "Prio=High",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Unknown field 'Prio'. Did you mean 'Priority'? Available fields: State, Priority, Type",
        ));

    assert!(!take_calls(&scenario).contains(&"create_issue".to_string()));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_value_added_after_caching_is_accepted_after_one_refetch() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);

    let fields = scenario.join("responses/project_custom_fields.json");
    let updated = fs::read_to_string(&fields)
        .unwrap()
        .replace("\"Improvement\"", "\"Improvement\", \"Spike\"");
    fs::write(&fields, updated).unwrap();

    track_mock(&dir, &scenario)
        .args(create_args("Type=Spike"))
        .assert()
        .success();
    assert_eq!(
        take_calls(&scenario),
        ["get_project_custom_fields", "create_issue"]
    );

    // The refetch was written back: the next one is a single call again.
    track_mock(&dir, &scenario)
        .args(create_args("Type=Spike"))
        .assert()
        .success();
    assert_eq!(take_calls(&scenario), ["create_issue"]);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn shorthand_flags_are_checked_only_when_it_is_free() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    let shorthand = [
        "issue",
        "create",
        "-p",
        "DEMO",
        "-s",
        "x",
        "--priority",
        "Bogus",
    ];

    // Cold: nothing in the old flow read the schema for a shorthand, and the
    // default check must not add a request. The value goes to the tracker.
    track_mock(&dir, &scenario)
        .args(shorthand)
        .assert()
        .success();
    assert_eq!(take_calls(&scenario), ["list_projects", "create_issue"]);

    // Warm: the schema is at hand, so the same command is refused.
    warm_cache(&dir, &scenario);
    track_mock(&dir, &scenario)
        .args(shorthand)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Invalid value 'Bogus' for field 'Priority'",
        ));
    assert!(!take_calls(&scenario).contains(&"create_issue".to_string()));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn no_validate_sends_fields_as_given() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");

    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "create",
            "-p",
            "DEMO",
            "-s",
            "x",
            "--field",
            "Prio=High",
            "--no-validate",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr
            .matches("field 'Prio' not found in project schema")
            .count(),
        1,
        "one warning, once: {stderr}"
    );
    assert!(take_calls(&scenario).contains(&"create_issue".to_string()));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn dry_run_checks_without_validate_and_never_writes() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");

    track_mock(&dir, &scenario)
        .args(create_args("Type=Bug"))
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Validation passed"));
    assert!(!take_calls(&scenario).contains(&"create_issue".to_string()));

    track_mock(&dir, &scenario)
        .args(create_args("Type=Nope"))
        .arg("--dry-run")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid value 'Nope'"));

    fs::remove_dir_all(&dir).unwrap();
}

// =============================================================================
// start / complete
// =============================================================================

#[test]
fn start_refuses_a_default_state_the_project_does_not_have() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-start-complete");

    // The backend default is "Develop"; this project has Open, In Progress, Done.
    track_mock(&dir, &scenario)
        .args(["issue", "start", "DEMO-1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Invalid value 'Develop' for field 'State'. Valid values: Open, In Progress, Done. \
             Pass --state <NAME> to choose one.",
        ));

    assert!(!take_calls(&scenario).contains(&"update_issue".to_string()));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn warm_start_with_a_valid_state_is_one_call() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-start-complete");
    warm_cache(&dir, &scenario);

    track_mock(&dir, &scenario)
        .args(["issue", "start", "DEMO-1", "--state", "In Progress"])
        .assert()
        .success()
        .stdout(predicate::str::contains("State=In Progress"));

    assert_eq!(take_calls(&scenario), ["update_issue"]);

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn complete_settles_on_the_only_resolved_state() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-start-complete");
    // No "Done" here: the one resolved state is called "Fixed".
    let fields = scenario.join("responses/project_custom_fields.json");
    let updated = fs::read_to_string(&fields)
        .unwrap()
        .replace("\"Done\"", "\"Fixed\"");
    fs::write(&fields, updated).unwrap();

    track_mock(&dir, &scenario)
        .args(["issue", "complete", "DEMO-3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("State=Fixed"));

    // An explicit state is never second-guessed into another one.
    track_mock(&dir, &scenario)
        .args(["issue", "complete", "DEMO-3", "--state", "Done"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Valid values: Open, In Progress, Fixed",
        ));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_explicit_field_is_sent_without_any_lookup() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-start-complete");

    track_mock(&dir, &scenario)
        .args([
            "issue", "start", "DEMO-2", "--field", "Stage", "--state", "Develop",
        ])
        .assert()
        .success();

    assert_eq!(take_calls(&scenario), ["update_issue"]);

    fs::remove_dir_all(&dir).unwrap();
}

// =============================================================================
// Writes keep the cache in step
// =============================================================================

#[test]
fn tag_changes_reach_the_cached_tag_list() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "tags-operations");
    track_mock(&dir, &scenario)
        .args(["cache", "refresh"])
        .assert()
        .success();
    let tags = dir.join(".tracker-cli/cache/youtrack/backend/tags.json");
    let before: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&tags).unwrap()).unwrap();
    let count = before.as_array().unwrap().len();

    track_mock(&dir, &scenario)
        .args(["tags", "list"])
        .assert()
        .success();

    let after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&tags).unwrap()).unwrap();
    assert_eq!(after.as_array().unwrap().len(), count);

    fs::remove_dir_all(&dir).unwrap();
}
