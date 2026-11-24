//! Lookups that read through the cache, validation that is on by default,
//! and `start`/`complete` checking the state before they write.
//!
//! Every test counts backend calls through the mock's call log: the point of
//! the cache is fewer of them, and the point of the safeguards is that a
//! cached "no" is checked with the backend before it becomes an error.

use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

mod common;
use common::{
    copy_dir_recursive, copy_scenario, mock_call_methods, mock_calls, read_json, temp_dir,
};

/// [`common::track_mock`] with DEMO as the default project, and so its cache
/// in `dir`.
fn track_mock(dir: &Path, scenario: &Path) -> assert_cmd::Command {
    let mut cmd = common::track_mock(dir, scenario);
    cmd.env("TRACKER_DEFAULT_PROJECT", "DEMO");
    cmd
}

/// The YouTrack cache of a `track` whose home is `dir`.
fn cache_root(dir: &Path) -> PathBuf {
    dir.join(".tracker-cli").join("cache").join("youtrack")
}

/// Backend methods called since the log was last cleared, then clear it.
fn take_calls(scenario: &Path) -> Vec<String> {
    let methods = mock_call_methods(scenario);
    fs::write(tracker_mock::call_log_path(scenario), "").unwrap();
    methods
        .into_iter()
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
    let mut value = read_json(path);
    walk(&mut value, timestamp);
    fs::write(path, serde_json::to_string(&value).unwrap()).unwrap();
}

#[test]
fn an_expired_schema_is_fetched_again() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    // Lookups trust cached answers for a day.
    let cache = cache_root(&dir);
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
}

#[test]
fn a_cache_from_another_instance_is_not_used() {
    let dir = temp_dir();
    let first = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &first);
    // The same fixture under another path stands for another tracker.
    let second = dir.join("other-instance");
    copy_dir_recursive(&first, &second);
    fs::write(tracker_mock::call_log_path(&second), "").unwrap();

    track_mock(&dir, &second)
        .args(create_args("Type=Bug"))
        .assert()
        .success();

    assert_eq!(
        take_calls(&second),
        ["list_projects", "get_project_custom_fields", "create_issue"]
    );
}

#[test]
fn reading_fields_refreshes_the_cached_schema() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    let schema = cache_root(&dir).join("projects/DEMO/schema.json");
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
    let tags = cache_root(&dir).join("backend/tags.json");
    let before = read_json(&tags);
    let count = before.as_array().unwrap().len();

    track_mock(&dir, &scenario)
        .args(["tags", "list"])
        .assert()
        .success();

    let after = read_json(&tags);
    assert_eq!(after.as_array().unwrap().len(), count);
}

// =============================================================================
// Project ids
// =============================================================================

/// Route `create_issue` for `project_id` to an error with `status`, ahead of
/// the scenario's own responses.
fn refuse_creates_in(scenario: &Path, project_id: &str, status: u16) {
    fs::write(
        scenario.join("responses/create_refused.json"),
        format!(r#"{{"message": "Entity with id {project_id} not found"}}"#),
    )
    .unwrap();
    let manifest = scenario.join("manifest.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        format!(
            "[[responses]]\nmethod = \"create_issue\"\nfile = \"create_refused.json\"\n\
             status = {status}\n[responses.args]\nproject = \"{project_id}\"\n\n{original}"
        ),
    )
    .unwrap();
}

/// The project every `create_issue` since the log was last cleared went to.
fn create_projects(scenario: &Path) -> Vec<String> {
    mock_calls(scenario)
        .into_iter()
        .filter(|entry| entry["method"] == "create_issue")
        .map(|entry| entry["args"]["project"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_project_recreated_under_a_new_id_is_found_again() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    // DEMO is deleted and recreated: the cache still says 0-1.
    let projects = scenario.join("responses/list_projects.json");
    let recreated = fs::read_to_string(&projects)
        .unwrap()
        .replace("\"0-1\"", "\"0-5\"");
    fs::write(&projects, recreated).unwrap();
    refuse_creates_in(&scenario, "0-1", 404);

    track_mock(&dir, &scenario)
        .args(create_args("Type=Bug"))
        .assert()
        .success();
    // The cached id was turned down, so it was asked for once more.
    assert_eq!(create_projects(&scenario), ["0-1", "0-5"]);
    assert_eq!(
        take_calls(&scenario),
        ["create_issue", "list_projects", "create_issue"]
    );

    // The cache learned the new id: the next run does not try the old one.
    track_mock(&dir, &scenario)
        .args(create_args("Type=Bug"))
        .assert()
        .success();
    assert_eq!(create_projects(&scenario), ["0-5"]);
    assert!(!take_calls(&scenario).contains(&"list_projects".to_string()));
}

#[test]
fn a_cached_id_that_was_right_is_not_written_to_twice() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    // The tracker refuses the create for another reason.
    refuse_creates_in(&scenario, "0-1", 400);

    track_mock(&dir, &scenario)
        .args(create_args("Type=Bug"))
        .assert()
        .failure();

    // One check that the id still holds, and no second create.
    assert_eq!(create_projects(&scenario), ["0-1"]);
    assert_eq!(take_calls(&scenario), ["create_issue", "list_projects"]);
}

#[test]
fn a_deleted_project_is_reported_and_forgotten() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    fs::write(scenario.join("responses/list_projects.json"), "[]").unwrap();
    refuse_creates_in(&scenario, "0-1", 404);

    track_mock(&dir, &scenario)
        .args(create_args("Type=Bug"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to resolve project 'DEMO'"));

    let resolved =
        fs::read_to_string(cache_root(&dir).join("backend/resolved_projects.json")).unwrap();
    assert!(!resolved.contains("demo"), "{resolved}");
}

#[test]
fn a_plan_resolves_each_project_once() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    let plan = dir.join("plan.json");
    let create = |summary: &str| {
        serde_json::json!({
            "op": "create_issue",
            "summary": summary,
            "fields": {"Priority": "Normal", "Type": "Bug"}
        })
    };
    fs::write(
        &plan,
        serde_json::json!({
            "version": 1,
            "defaults": {"project": "DEMO"},
            "operations": [
                create("Fix login timeout bug"),
                create("Add remember me checkbox"),
                create("Update password validation"),
            ]
        })
        .to_string(),
    )
    .unwrap();

    // Without the cache every answer comes from the backend, once.
    track_mock(&dir, &scenario)
        .arg("--no-cache")
        .arg("apply")
        .arg(&plan)
        .assert()
        .success();

    assert_eq!(
        take_calls(&scenario),
        [
            "list_projects",
            "get_project_custom_fields",
            "create_issue",
            "create_issue",
            "create_issue"
        ]
    );
}

#[test]
fn no_cache_env_values_that_mean_off_keep_the_cache() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);

    for value in ["0", "false", "off", ""] {
        track_mock(&dir, &scenario)
            .env("TRACK_NO_CACHE", value)
            .args(create_args("Type=Bug"))
            .assert()
            .success();
        assert_eq!(
            take_calls(&scenario),
            ["create_issue"],
            "TRACK_NO_CACHE={value:?}"
        );
    }
}

#[test]
fn changes_reach_the_cache_even_without_cached_lookups() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "project-operations");
    warm_cache(&dir, &scenario);

    track_mock(&dir, &scenario)
        .args([
            "--no-cache",
            "project",
            "create",
            "-n",
            "Test Project",
            "-s",
            "TEST",
        ])
        .assert()
        .success();

    // `--no-cache` is about answering lookups; a cache that missed the new
    // project would say it does not exist.
    let meta = cache_root(&dir).join("projects/TEST/meta.json");
    let meta = read_json(&meta);
    assert_eq!(meta["id"], "0-3");
}

#[test]
fn project_fields_uses_the_cached_project_id() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);

    track_mock(&dir, &scenario)
        .args(["project", "fields", "DEMO"])
        .assert()
        .success();

    // The schema is read live, the id is not looked up again.
    assert_eq!(take_calls(&scenario), ["get_project_custom_fields"]);
}

#[test]
fn a_failed_project_fields_names_the_project_as_given() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    fail_with(&scenario, "get_project_custom_fields", 403);

    let output = track_mock(&dir, &scenario)
        .args(["project", "fields", "DEMO"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    // DEMO, as typed, not the internal id the cache resolved it to.
    assert!(
        stderr.contains("Failed to fetch custom fields for project 'DEMO'"),
        "{stderr}"
    );
    assert!(!stderr.contains("'0-1'"), "{stderr}");
}

// =============================================================================
// Second opinions, and when not to ask for one
// =============================================================================

/// Put `mapping` (manifest TOML) ahead of the scenario's own responses.
fn answer_first(scenario: &Path, mapping: &str) {
    let manifest = scenario.join("manifest.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, format!("{mapping}\n\n{original}")).unwrap();
}

/// Answer `method` with an error of `status`.
fn fail_with(scenario: &Path, method: &str, status: u16) {
    let file = format!("{method}_{status}.json");
    fs::write(
        scenario.join("responses").join(&file),
        format!(r#"{{"message": "HTTP {status} from {method}"}}"#),
    )
    .unwrap();
    answer_first(
        scenario,
        &format!("[[responses]]\nmethod = \"{method}\"\nfile = \"{file}\"\nstatus = {status}\n"),
    );
}

const UPDATE: [&str; 5] = ["issue", "update", "DEMO-10", "--field", "Priority=High"];

#[test]
fn a_dry_run_that_cannot_read_the_schema_fails_instead_of_passing() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    fail_with(&scenario, "get_project_custom_fields", 403);

    // Nothing could be checked, so a dry run cannot say the check passed.
    let output = track_mock(&dir, &scenario)
        .args(create_args("Priorty=High"))
        .arg("--dry-run")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(4), "{stdout}{stderr}");
    assert!(!stdout.contains("Validation passed"), "{stdout}");
    assert!(
        stderr.contains("Cannot check the custom fields without the project schema"),
        "{stderr}"
    );
    assert!(!take_calls(&scenario).contains(&"create_issue".to_string()));

    // The write itself is not stopped for want of a schema: the tracker
    // decides, as the default mode promises.
    track_mock(&dir, &scenario)
        .args(create_args("Type=Bug"))
        .assert()
        .success();
    assert!(take_calls(&scenario).contains(&"create_issue".to_string()));
}

#[test]
fn a_dry_run_plan_reports_an_update_of_an_issue_that_does_not_exist() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    // DEMO is cached, so the project of DEMO-9999 can be read off its id.
    warm_cache(&dir, &scenario);
    let plan = dir.join("plan.json");
    fs::write(
        &plan,
        serde_json::json!({
            "version": 1,
            "operations": [
                {"op": "update_issue", "issue": "DEMO-9999", "fields": {"Priority": "High"}}
            ]
        })
        .to_string(),
    )
    .unwrap();

    // In the default mode and the strict one alike.
    for validation in [&[][..], &["--validate"][..]] {
        let output = track_mock(&dir, &scenario)
            .args(["-o", "json", "apply"])
            .arg(&plan)
            .arg("--dry-run")
            .args(validation)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{validation:?}");
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["results"][0]["status"], "failed");
        let error = report["results"][0]["error"].as_str().unwrap();
        assert!(
            error.contains("Failed to fetch issue 'DEMO-9999'"),
            "{error}"
        );
    }
    assert!(!take_calls(&scenario).contains(&"update_issue".to_string()));
}

#[test]
fn a_failed_update_is_rechecked_only_when_the_tracker_could_answer() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    answer_first(
        &scenario,
        "[[responses]]\nmethod = \"get_issue\"\nfile = \"create_issue_1.json\"\n",
    );

    // Unreachable or rate limited: asking where the issue is would fail the
    // same way, so the failure is reported after one request.
    fail_with(&scenario, "update_issue", 503);
    track_mock(&dir, &scenario).args(UPDATE).assert().code(7);
    assert_eq!(take_calls(&scenario), ["update_issue"]);

    // Refused as put: the issue may have moved, so the tracker is asked. It
    // is still in DEMO, so the refusal stands and nothing is sent again.
    fail_with(&scenario, "update_issue", 400);
    track_mock(&dir, &scenario).args(UPDATE).assert().failure();
    assert_eq!(take_calls(&scenario), ["update_issue", "get_issue"]);
}

#[test]
fn fields_refused_for_a_moved_issue_are_checked_against_its_new_project() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");
    warm_cache(&dir, &scenario);
    // DEMO-10 has moved to project 0-2, whose schema has a Severity field.
    let responses = scenario.join("responses");
    let moved = fs::read_to_string(responses.join("create_issue_1.json"))
        .unwrap()
        .replace("\"0-1\"", "\"0-2\"");
    fs::write(responses.join("moved_issue.json"), moved).unwrap();
    fs::write(
        responses.join("ops_fields.json"),
        r#"[{"id": "f-9", "name": "Severity", "field_type": "enum", "required": false,
            "values": ["Low", "High"]}]"#,
    )
    .unwrap();
    answer_first(
        &scenario,
        "[[responses]]\nmethod = \"get_issue\"\nfile = \"moved_issue.json\"\n\n\
         [[responses]]\nmethod = \"update_issue\"\nfile = \"moved_issue.json\"\n\n\
         [[responses]]\nmethod = \"get_project_custom_fields\"\nfile = \"ops_fields.json\"\n\
         [responses.args]\nproject_id = \"0-2\"\n",
    );

    track_mock(&dir, &scenario)
        .args(["issue", "update", "DEMO-10", "--field", "Severity=High"])
        .assert()
        .success();

    // DEMO's cached schema has no Severity, and neither has its live one.
    // Then the tracker says where the issue is, and that project's schema
    // takes the update.
    assert_eq!(
        take_calls(&scenario),
        [
            "get_project_custom_fields",
            "get_issue",
            "get_project_custom_fields",
            "update_issue"
        ]
    );
}
