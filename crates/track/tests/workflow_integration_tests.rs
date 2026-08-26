//! Failing CLI contract tests for the first workflow-pack slice (#294).
//!
//! Spec: `docs/design/294-workflow-query-packs.md` (design-only PR #352).
//! These tests encode pack selection, collision, validation, and offline
//! `workflow` commands. They must fail on current main and go green when
//! the slice is implemented. No production code lives here.

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

// =============================================================================
// Helpers (mirrors command_integration_tests.rs / cli_tests.rs)
// =============================================================================

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
        "track-workflow-test-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
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

/// Isolated CLI invocation: HOME is the temp dir, TRACKER_CONFIG is unset
/// unless a later `.env("TRACKER_CONFIG", ...)` overrides it.
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

fn track_mock(dir: &Path, scenario: &Path) -> assert_cmd::Command {
    let mut cmd = track_in(dir);
    cmd.env("TRACK_MOCK_DIR", scenario.to_str().unwrap()).args([
        "--url",
        "https://mock.test",
        "--token",
        "mock-token",
    ]);
    cmd
}

fn write_config(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn combined_output(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn parse_json_stdout(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON ({e}): {stdout}"))
}

fn search_queries(scenario: &Path) -> Vec<String> {
    let log = fs::read_to_string(scenario.join("call_log.jsonl")).unwrap_or_default();
    log.lines()
        .filter_map(|line| {
            let entry: Value = serde_json::from_str(line).ok()?;
            if entry["method"].as_str() == Some("search_issues") {
                entry["args"]["query"].as_str().map(str::to_string)
            } else {
                None
            }
        })
        .collect()
}

fn json_mentions_name(value: &Value, name: &str) -> bool {
    match value {
        Value::String(s) => s == name,
        Value::Object(map) => {
            if map.get("name").and_then(|v| v.as_str()) == Some(name) {
                return true;
            }
            map.values().any(|v| json_mentions_name(v, name))
        }
        Value::Array(items) => items.iter().any(|v| json_mentions_name(v, name)),
        _ => false,
    }
}

/// Seed a local cache so built-in `-T unresolved` can resolve today.
/// Requires `.track.toml` in `dir` so the cache is `./.tracker-cache/`.
fn seed_youtrack_builtin_cache(dir: &Path) {
    let cache = dir.join(".tracker-cache");
    fs::create_dir_all(cache.join("backend")).unwrap();
    fs::write(
        cache.join("index.json"),
        r#"{
  "version": 2,
  "updated_at": "2026-01-01T00:00:00Z",
  "backend_metadata": {
    "backend_type": "youtrack",
    "base_url": "https://mock.test"
  },
  "default_project": "DEMO"
}
"#,
    )
    .unwrap();
    fs::write(
        cache.join("backend/query_templates.json"),
        r#"[
  {
    "name": "unresolved",
    "description": "All unresolved issues in project",
    "query": "project: {PROJECT} #Unresolved",
    "backend": "youtrack"
  }
]
"#,
    )
    .unwrap();
}

fn ready_pack_toml() -> &'static str {
    r#"backend = "youtrack"

[workflow_pack]
name = "Orek backlog"
description = "Project-local views for backlog work."
default_project = "DEMO"

[[workflow_pack.queries]]
name = "ready"
description = "Issues ready for implementation."
query = "project: {PROJECT} #Unresolved State: {Ready}"

[[workflow_pack.queries]]
name = "blocked"
description = "Issues blocked by dependencies."
query = "project: {PROJECT} #Unresolved tag: blocked"
"#
}

fn collision_pack_toml() -> &'static str {
    r#"backend = "youtrack"

[workflow_pack]
name = "Collision pack"
description = "Shadows the built-in unresolved template."

[[workflow_pack.queries]]
name = "unresolved"
description = "Repo-local unresolved view."
query = "project: {PROJECT} #Unresolved State: {Ready}"
"#
}

// =============================================================================
// 1. Pack query selected by -T
// =============================================================================

#[test]
fn pack_query_selected_by_template_flag() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    let pack = dir.join("pack.toml");
    write_config(&pack, ready_pack_toml());

    let output = track_mock(&dir, &scenario)
        .args(["--config"])
        .arg(&pack)
        .args(["issue", "search", "-T", "ready", "-p", "DEMO", "-o", "json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "pack query `-T ready` must succeed, got: {}",
        combined_output(&output)
    );

    let queries = search_queries(&scenario);
    assert!(
        queries
            .iter()
            .any(|q| q.contains("State: {Ready}") && q.contains("project: DEMO")),
        "search must use the pack query (State: {{Ready}} after {{PROJECT}} expansion), got {queries:?}"
    );
    assert!(
        !queries.iter().any(|q| q == "project: DEMO #Unresolved"),
        "must not fall back to the built-in unresolved query, got {queries:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// 2. Pack wins on name collision + validate warns
// =============================================================================

#[test]
fn pack_wins_on_name_collision_and_validate_warns() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    write_config(&dir.join(".track.toml"), collision_pack_toml());
    seed_youtrack_builtin_cache(&dir);

    let search = track_mock(&dir, &scenario)
        .args([
            "issue",
            "search",
            "-T",
            "unresolved",
            "-p",
            "DEMO",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        search.status.success(),
        "`-T unresolved` must succeed using the pack query, got: {}",
        combined_output(&search)
    );

    let queries = search_queries(&scenario);
    assert!(
        queries
            .iter()
            .any(|q| q.contains("State: {Ready}") && q.contains("project: DEMO")),
        "pack must win the unresolved collision, got {queries:?}"
    );
    assert!(
        !queries.iter().any(|q| q == "project: DEMO #Unresolved"),
        "built-in unresolved query must not be used when a pack query shadows it, got {queries:?}"
    );

    let validate = track_in(&dir)
        .args(["-o", "json", "workflow", "validate"])
        .output()
        .unwrap();
    assert!(
        validate.status.success(),
        "collision is a warning, not an error; validate must exit 0, got: {}",
        combined_output(&validate)
    );

    let json = parse_json_stdout(&validate);
    assert_eq!(
        json["valid"], true,
        "validate can stay valid:true on a shadows_builtin warning: {json}"
    );
    let warnings = json["warnings"]
        .as_array()
        .unwrap_or_else(|| panic!("validate JSON must include warnings array: {json}"));
    assert!(
        warnings.iter().any(|w| {
            w["code"].as_str() == Some("shadows_builtin")
                && w["query"]
                    .as_str()
                    .is_some_and(|q| q.eq_ignore_ascii_case("unresolved"))
        }),
        "expected shadows_builtin warning for unresolved, got {json}"
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// 3. Duplicate names in one pack are an error
// =============================================================================

#[test]
fn duplicate_query_names_in_one_pack_are_an_error() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    write_config(
        &dir.join(".track.toml"),
        r#"backend = "youtrack"

[workflow_pack]
name = "Duplicate pack"
description = "Two queries that collide case-insensitively."

[[workflow_pack.queries]]
name = "ready"
description = "Lowercase ready."
query = "project: {PROJECT} #Unresolved State: {Ready}"

[[workflow_pack.queries]]
name = "Ready"
description = "Case-variant ready."
query = "project: {PROJECT} Type: {Task}"
"#,
    );

    let output = track_mock(&dir, &scenario)
        .args(["issue", "search", "-T", "ready", "-p", "DEMO", "-o", "json"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "duplicate pack query names must fail a pack-consuming command; no silent first-wins. got: {}",
        combined_output(&output)
    );

    let text = combined_output(&output).to_ascii_lowercase();
    assert!(
        text.contains("ready"),
        "error must name the duplicate query, got: {}",
        combined_output(&output)
    );
    assert!(
        text.contains("duplicate") || text.contains("duplicated") || text.contains("already"),
        "error must identify the names as a duplicate, got: {}",
        combined_output(&output)
    );
    assert!(
        search_queries(&scenario).is_empty(),
        "must not silently run a first-wins search, got {:?}",
        search_queries(&scenario)
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// 4. Invalid pack fails a pack-consuming command (no silent fallback)
// =============================================================================

#[test]
fn invalid_pack_fails_search_without_builtin_fallback() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    write_config(
        &dir.join(".track.toml"),
        r#"backend = "youtrack"

[workflow_pack]
name = "Broken pack"
description = "Unknown field must be rejected."
not_a_real_pack_field = "typo"

[[workflow_pack.queries]]
name = "unresolved"
description = "Would shadow built-in if the pack were accepted."
query = "project: {PROJECT} #Unresolved State: {Ready}"
"#,
    );
    seed_youtrack_builtin_cache(&dir);

    let output = track_mock(&dir, &scenario)
        .args([
            "issue",
            "search",
            "-T",
            "unresolved",
            "-p",
            "DEMO",
            "-o",
            "json",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "an invalid pack must fail pack-consuming search; must not silently run built-in unresolved. got: {}",
        combined_output(&output)
    );

    let text = combined_output(&output);
    let lower = text.to_ascii_lowercase();
    assert!(
        lower.contains("not_a_real_pack_field")
            || lower.contains("unknown")
            || lower.contains("workflow_pack"),
        "error must blame the invalid pack, got: {text}"
    );
    assert!(
        !search_queries(&scenario)
            .iter()
            .any(|q| q == "project: DEMO #Unresolved"),
        "must not fall back to the built-in unresolved query, got {:?}",
        search_queries(&scenario)
    );

    let _ = fs::remove_dir_all(&dir);
}

// =============================================================================
// 5. workflow list|show|validate work offline from the pack
// =============================================================================

#[test]
fn workflow_list_show_validate_work_offline_from_pack() {
    let dir = temp_dir();
    let pack = dir.join("pack.toml");
    write_config(&pack, ready_pack_toml());

    let list = track_in(&dir)
        .args(["-o", "json", "--config"])
        .arg(&pack)
        .args(["workflow", "list"])
        .output()
        .unwrap();
    assert!(
        list.status.success(),
        "`workflow list` must work offline from --config, got: {}",
        combined_output(&list)
    );
    let list_json = parse_json_stdout(&list);
    assert!(
        json_mentions_name(&list_json, "ready") && json_mentions_name(&list_json, "blocked"),
        "workflow list must include pack query names, got {list_json}"
    );

    let show = track_in(&dir)
        .args(["-o", "json", "--config"])
        .arg(&pack)
        .args(["workflow", "show"])
        .output()
        .unwrap();
    assert!(
        show.status.success(),
        "`workflow show` must work offline from --config, got: {}",
        combined_output(&show)
    );
    let show_json = parse_json_stdout(&show);
    let show_text = show_json.to_string();
    assert!(
        show_text.contains("Orek backlog"),
        "workflow show must include the pack name, got {show_json}"
    );
    assert!(
        json_mentions_name(&show_json, "ready"),
        "workflow show must include pack queries, got {show_json}"
    );
    assert!(
        show_text.contains("explicit") || show_text.contains("repo") || show_text.contains("user"),
        "workflow show must include pack source, got {show_json}"
    );

    let validate_ok = track_in(&dir)
        .args(["-o", "json", "--config"])
        .arg(&pack)
        .args(["workflow", "validate"])
        .output()
        .unwrap();
    assert!(
        validate_ok.status.success(),
        "`workflow validate` must run without tracker credentials, got: {}",
        combined_output(&validate_ok)
    );
    let validate_json = parse_json_stdout(&validate_ok);
    assert_eq!(
        validate_json["valid"], true,
        "valid pack must report valid:true: {validate_json}"
    );

    let broken = dir.join("broken.toml");
    write_config(
        &broken,
        r#"backend = "youtrack"

[workflow_pack]
name = ""
description = "Empty pack name is a semantic error."

[[workflow_pack.queries]]
name = "ready"
description = "Issues ready for implementation."
query = "project: {PROJECT} #Unresolved State: {Ready}"
"#,
    );
    let validate_err = track_in(&dir)
        .args(["-o", "json", "--config"])
        .arg(&broken)
        .args(["workflow", "validate"])
        .output()
        .unwrap();
    assert!(
        !validate_err.status.success(),
        "semantic pack errors must exit nonzero, got: {}",
        combined_output(&validate_err)
    );
    let err_text = combined_output(&validate_err).to_ascii_lowercase();
    assert!(
        err_text.contains("name")
            && (err_text.contains("empty")
                || err_text.contains("non-empty")
                || err_text.contains("required")
                || err_text.contains("invalid")),
        "validate must name the empty pack name, got: {}",
        combined_output(&validate_err)
    );

    let _ = fs::remove_dir_all(&dir);
}
