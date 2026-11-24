//! Integration tests for the sharded cache system (v2)
//!
//! These tests exercise cache commands end-to-end using the mock backend.
//! Each test that writes cache files uses a unique temp directory to avoid
//! interference between tests.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

mod common;
use common::{copy_scenario, fixtures_path, read_json, temp_dir};

/// Get path to the cache-operations scenario
fn cache_scenario() -> PathBuf {
    fixtures_path().join("cache-operations")
}

/// Ensure a .track.toml exists in the temp directory so the cache resolves locally.
fn ensure_project_context(dir: &Path) {
    let track_toml = dir.join(".track.toml");
    if !track_toml.exists() {
        fs::write(&track_toml, "# test project context\n").unwrap();
    }
}

/// Build a track command pointed at a temp directory with mock backend enabled.
/// Passes dummy --url and --token since config validation runs before mock activation.
/// Creates a .track.toml to ensure project context (local cache).
fn track_in(dir: &Path) -> Command {
    ensure_project_context(dir);
    common::track_mock(dir, &cache_scenario())
}

/// Build a track command pointed at a temp directory WITHOUT mock (for reading cache only).
/// Uses a dummy config so validation doesn't require a real server.
/// Creates a .track.toml to ensure project context (local cache).
fn track_in_no_mock(dir: &Path) -> Command {
    ensure_project_context(dir);
    let mut cmd = common::track_in(dir);
    cmd.args(["--url", "https://mock.test", "--token", "mock-token"]);
    cmd
}

/// Run `track cache refresh` in the given directory to populate cache for subsequent tests.
fn populate_cache(dir: &Path) {
    track_in(dir).args(["cache", "refresh"]).assert().success();
}

/// A private copy of the scenario with the cache in `dir` filled from it
/// and its call log emptied, so a test sees only the calls that follow.
/// The shared fixture would mix in other tests' calls.
fn warm_cache(dir: &Path) -> PathBuf {
    ensure_project_context(dir);
    let scenario = copy_scenario(dir, "cache-operations");
    common::track_mock(dir, &scenario)
        .args(["cache", "refresh"])
        .assert()
        .success();
    tracker_mock::clear_call_log(&scenario).unwrap();
    scenario
}

/// Assert that the only entry since [`warm_cache`] is the CLI's own record
/// of `context`: the command answered from the cache and fetched nothing.
fn assert_answered_from_cache(scenario: &Path) {
    assert_eq!(
        common::mock_call_methods(scenario),
        ["context"],
        "context fetched from the backend despite a cache for this instance"
    );
}

/// Where `track` keeps the YouTrack cache of the project in `dir`.
fn cache_root(dir: &Path) -> PathBuf {
    dir.join(".tracker-cache").join("youtrack")
}

/// The cache file `rel` (relative to [`cache_root`]), parsed.
fn shard(dir: &Path, rel: &str) -> serde_json::Value {
    read_json(&cache_root(dir).join(rel))
}

/// Make the cache in `dir` look as if it was written for another instance
/// of the same backend (the config's URL changed since, or the cache was
/// committed by someone using another tracker). Returns the index path.
fn mark_cache_as_another_instance(dir: &Path) -> PathBuf {
    let index = cache_root(dir).join("index.json");
    let mut json = read_json(&index);
    json["backend_metadata"]["scope"] = "https://elsewhere.test|".into();
    fs::write(&index, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    index
}

fn cache_scope_in(index: &Path) -> String {
    read_json(index)["backend_metadata"]["scope"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

// =============================================================================
// cache refresh
// =============================================================================

#[test]
fn test_cache_refresh_creates_sharded_directory_layout() {
    let dir = temp_dir();
    populate_cache(&dir);

    let cache_dir = cache_root(&dir);

    // Index
    assert!(
        cache_dir.join("index.json").exists(),
        "index.json should exist"
    );

    // Backend shards
    assert!(
        cache_dir.join("backend/tags.json").exists(),
        "tags.json should exist"
    );
    assert!(
        cache_dir.join("backend/query_templates.json").exists(),
        "query_templates.json should exist"
    );
    assert!(
        cache_dir.join("backend/link_types.json").exists(),
        "link_types.json should exist"
    );

    // Project shards (DEMO and OTHER from fixture)
    assert!(
        cache_dir.join("projects/DEMO/meta.json").exists(),
        "DEMO meta.json should exist"
    );
    assert!(
        cache_dir.join("projects/DEMO/fields.json").exists(),
        "DEMO fields.json should exist"
    );
    assert!(
        cache_dir.join("projects/DEMO/users.json").exists(),
        "DEMO users.json should exist"
    );
    assert!(
        cache_dir.join("projects/DEMO/workflow.json").exists(),
        "DEMO workflow.json should exist"
    );
    assert!(
        cache_dir.join("projects/OTHER/meta.json").exists(),
        "OTHER meta.json should exist"
    );

    // KB shards
    assert!(
        cache_dir.join("kb/articles.json").exists(),
        "articles.json should exist"
    );
    assert!(
        cache_dir.join("kb/tree.json").exists(),
        "tree.json should exist"
    );

    // Runtime shards
    assert!(
        cache_dir.join("runtime/recent_issues.json").exists(),
        "recent_issues.json should exist"
    );
}

#[test]
fn test_cache_refresh_index_contains_version_and_metadata() {
    let dir = temp_dir();
    populate_cache(&dir);

    let index = shard(&dir, "index.json");

    assert_eq!(index["version"], 3);
    assert!(index["updated_at"].is_string());
    assert_eq!(index["backend_metadata"]["backend_type"], "youtrack");
}

#[test]
fn test_cache_refresh_project_shard_content() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Check project meta
    let meta = shard(&dir, "projects/DEMO/meta.json");
    assert_eq!(meta["short_name"], "DEMO");
    assert_eq!(meta["name"], "Demo Project");
    assert_eq!(meta["id"], "0-1");

    // Check fields
    let fields = shard(&dir, "projects/DEMO/fields.json");
    let fields_arr = fields.as_array().unwrap();
    assert!(
        fields_arr.len() >= 3,
        "Should have at least State, Priority, Type fields"
    );

    let field_names: Vec<&str> = fields_arr
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(field_names.contains(&"State"), "Should have State field");
    assert!(
        field_names.contains(&"Priority"),
        "Should have Priority field"
    );

    // Check users
    let users = shard(&dir, "projects/DEMO/users.json");
    let users_arr = users.as_array().unwrap();
    assert_eq!(users_arr.len(), 2);
    assert_eq!(users_arr[0]["login"], "john.doe");

    // Check workflow
    let workflow = shard(&dir, "projects/DEMO/workflow.json");
    assert_eq!(workflow["project_short_name"], "DEMO");
    let state_fields = workflow["state_fields"].as_array().unwrap();
    assert!(
        !state_fields.is_empty(),
        "Should have state field workflows"
    );
}

#[test]
fn test_cache_refresh_backend_shard_content() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Tags
    let tags = shard(&dir, "backend/tags.json");
    let tag_names: Vec<&str> = tags
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(tag_names.contains(&"backend"));
    assert!(tag_names.contains(&"frontend"));
    assert!(tag_names.contains(&"urgent"));

    // Link types
    let link_types = shard(&dir, "backend/link_types.json");
    assert!(
        !link_types.as_array().unwrap().is_empty(),
        "Should have link types"
    );

    // Query templates
    let templates = shard(&dir, "backend/query_templates.json");
    let template_names: Vec<&str> = templates
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(template_names.contains(&"unresolved"));
    assert!(template_names.contains(&"bugs"));
}

#[test]
fn test_cache_refresh_text_output() {
    let dir = temp_dir();
    track_in(&dir)
        .args(["cache", "refresh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cache refreshed"))
        .stdout(predicate::str::contains("Projects"))
        .stdout(predicate::str::contains("Tags"));
}

#[test]
fn test_cache_refresh_json_output() {
    let dir = temp_dir();
    track_in(&dir)
        .args(["-o", "json", "cache", "refresh"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""success": true"#));
}

// =============================================================================
// cache status
// =============================================================================

#[test]
fn test_cache_status_empty() {
    let dir = temp_dir();

    // No cache exists yet
    track_in_no_mock(&dir)
        .args(["cache", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("empty"));
}

#[test]
fn test_cache_status_after_refresh() {
    let dir = temp_dir();
    populate_cache(&dir);

    track_in_no_mock(&dir)
        .args(["cache", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cache status"))
        .stdout(predicate::str::contains("Projects"))
        .stdout(predicate::str::contains("Tags"));
}

#[test]
fn test_cache_status_json_output() {
    let dir = temp_dir();
    populate_cache(&dir);

    let output = track_in_no_mock(&dir)
        .args(["-o", "json", "cache", "status"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(json["exists"], true);
    assert!(json["updated_at"].is_string());
    assert!(json["age_seconds"].is_number());
    assert!(json["projects_count"].as_u64().unwrap() >= 2);
    assert!(json["tags_count"].as_u64().unwrap() >= 3);
}

// =============================================================================
// cache show
// =============================================================================

#[test]
fn test_cache_show_json_contains_all_shards() {
    let dir = temp_dir();
    populate_cache(&dir);

    let output = track_in_no_mock(&dir)
        .args(["-o", "json", "cache", "show"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Projects
    let projects = json["projects"].as_array().unwrap();
    assert!(projects.len() >= 2);
    let project_names: Vec<&str> = projects
        .iter()
        .map(|p| p["short_name"].as_str().unwrap())
        .collect();
    assert!(project_names.contains(&"DEMO"));
    assert!(project_names.contains(&"OTHER"));

    // Tags
    assert!(json["tags"].as_array().unwrap().len() >= 3);

    // Fields
    assert!(!json["project_fields"].as_array().unwrap().is_empty());

    // Query templates
    assert!(!json["query_templates"].as_array().unwrap().is_empty());

    // Link types
    assert!(!json["link_types"].as_array().unwrap().is_empty());

    // Workflow hints
    assert!(!json["workflow_hints"].as_array().unwrap().is_empty());

    // Articles
    assert!(!json["articles"].as_array().unwrap().is_empty());
}

#[test]
fn test_cache_show_text_output() {
    let dir = temp_dir();
    populate_cache(&dir);

    track_in_no_mock(&dir)
        .args(["cache", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DEMO"))
        .stdout(predicate::str::contains("Demo Project"));
}

// =============================================================================
// cache path
// =============================================================================

#[test]
fn test_cache_path_reports_directory() {
    let dir = temp_dir();

    let output = track_in_no_mock(&dir)
        .args(["cache", "path"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let path_str = String::from_utf8(output.stdout).unwrap();
    let path_str = path_str.trim();

    assert!(
        std::path::Path::new(path_str).ends_with(".tracker-cache/youtrack"),
        "cache path should end with .tracker-cache/youtrack, got: {}",
        path_str
    );
}

#[test]
fn test_cache_dir_env_overrides_project_and_global_cache() {
    let dir = temp_dir();
    ensure_project_context(&dir);
    let cache_dir = dir.join("elsewhere");

    track_in(&dir)
        .env("TRACK_CACHE_DIR", &cache_dir)
        .args(["cache", "refresh"])
        .assert()
        .success();

    assert!(cache_dir.join("youtrack/index.json").is_file());
    assert!(
        !dir.join(".tracker-cache").exists(),
        "the project cache must stay untouched when TRACK_CACHE_DIR is set"
    );

    let output = track_in_no_mock(&dir)
        .env("TRACK_CACHE_DIR", &cache_dir)
        .args(["cache", "path"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        cache_dir.join("youtrack").to_string_lossy()
    );
}

#[test]
fn test_cache_path_json_output() {
    // Used to print the bare path whatever the output format.
    let dir = temp_dir();

    let output = track_in_no_mock(&dir)
        .args(["cache", "path", "-o", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("cache path -o json must print JSON ({e}): {output:?}"));
    let path = json["path"].as_str().unwrap();
    assert!(
        std::path::Path::new(path).ends_with(".tracker-cache/youtrack"),
        "cache path should end with .tracker-cache/youtrack, got: {}",
        path
    );
}

// =============================================================================
// cache refresh --if-stale
// =============================================================================

#[test]
fn test_cache_refresh_if_stale_skips_fresh_cache() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Cache was just created, 1d staleness threshold should skip. The run
    // uses the same (mock) instance the cache was written for.
    track_in(&dir)
        .args(["cache", "refresh", "--if-stale", "1d"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fresh"));
}

#[test]
fn test_cache_refresh_if_stale_json_output() {
    let dir = temp_dir();
    populate_cache(&dir);

    let output = track_in(&dir)
        .args(["-o", "json", "cache", "refresh", "--if-stale", "1d"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(json["skipped"], true);
    assert!(json["age_seconds"].is_number());
}

#[test]
fn test_cache_refresh_if_stale_refreshes_a_cache_for_another_instance() {
    let dir = temp_dir();
    populate_cache(&dir);
    let index = mark_cache_as_another_instance(&dir);

    // Minutes old, but not this instance's: it is refreshed, not kept.
    let output = track_in(&dir)
        .args(["-o", "json", "cache", "refresh", "--if-stale", "1d"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.get("skipped").is_none(), "{json}");
    assert!(cache_scope_in(&index).starts_with("mock:"));
}

// =============================================================================
// context command
// =============================================================================

#[test]
fn test_context_loads_from_cache() {
    let dir = temp_dir();
    let scenario = warm_cache(&dir);

    // Context loads the existing cache written for this (mock) instance
    common::track_mock(&dir, &scenario)
        .args(["context"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DEMO"))
        .stdout(predicate::str::contains("Projects"));
    assert_answered_from_cache(&scenario);
}

#[test]
fn test_context_json_output_from_cache() {
    let dir = temp_dir();
    let scenario = warm_cache(&dir);

    let output = common::track_mock(&dir, &scenario)
        .args(["-o", "json", "context"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_answered_from_cache(&scenario);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert!(json["projects"].as_array().unwrap().len() >= 2);
    assert!(!json["project_fields"].as_array().unwrap().is_empty());
    assert!(!json["tags"].as_array().unwrap().is_empty());
    assert!(!json["query_templates"].as_array().unwrap().is_empty());
    assert!(!json["workflow_hints"].as_array().unwrap().is_empty());
}

#[test]
fn test_context_project_filter() {
    let dir = temp_dir();
    let scenario = warm_cache(&dir);

    let output = common::track_mock(&dir, &scenario)
        .args(["-o", "json", "context", "--project", "DEMO"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_answered_from_cache(&scenario);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Only DEMO project should be present
    let projects = json["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["short_name"], "DEMO");
}

#[test]
fn test_context_refetches_a_cache_written_for_another_instance() {
    let dir = temp_dir();
    populate_cache(&dir);
    let index = mark_cache_as_another_instance(&dir);

    let output = track_in(&dir)
        .args(["-o", "json", "context"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // What it reports was fetched for this instance, and the cache with it.
    assert!(
        json["backend"]["scope"]
            .as_str()
            .is_some_and(|scope| scope.starts_with("mock:")),
        "{json}"
    );
    assert!(cache_scope_in(&index).starts_with("mock:"));
}

// =============================================================================
// Issue access recording (runtime shard isolation)
// =============================================================================

#[test]
fn test_issue_access_preserves_other_shards() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Read all shard files before issue access
    let tags_before = fs::read_to_string(cache_root(&dir).join("backend/tags.json")).unwrap();
    let fields_before =
        fs::read_to_string(cache_root(&dir).join("projects/DEMO/fields.json")).unwrap();
    let meta_before = fs::read_to_string(cache_root(&dir).join("projects/DEMO/meta.json")).unwrap();

    // Access an issue (triggers record_issue_access + save_runtime)
    let scenario = copy_scenario(&dir, "basic-workflow");
    common::track_mock(&dir, &scenario)
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .success();

    // Verify other shards were NOT modified
    let tags_after = fs::read_to_string(cache_root(&dir).join("backend/tags.json")).unwrap();
    let fields_after =
        fs::read_to_string(cache_root(&dir).join("projects/DEMO/fields.json")).unwrap();
    let meta_after = fs::read_to_string(cache_root(&dir).join("projects/DEMO/meta.json")).unwrap();

    assert_eq!(
        tags_before, tags_after,
        "tags.json should not change on issue access"
    );
    assert_eq!(
        fields_before, fields_after,
        "fields.json should not change on issue access"
    );
    assert_eq!(
        meta_before, meta_after,
        "meta.json should not change on issue access"
    );

    // But runtime shard should have the accessed issue
    let recent = shard(&dir, "runtime/recent_issues.json");
    let recent_arr = recent.as_array().unwrap();
    assert!(
        !recent_arr.is_empty(),
        "recent_issues should contain the accessed issue"
    );
    assert_eq!(recent_arr[0]["id_readable"], "DEMO-1");

    // Verify the compact runtime shard is loadable through normal cache loading.
    let output = track_in_no_mock(&dir)
        .args(["-o", "json", "cache", "show"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cache show should load runtime shard, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let loaded: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(loaded["recent_issues"][0]["id_readable"], "DEMO-1");
}

// =============================================================================
// Corruption resilience
// =============================================================================

#[test]
fn test_corrupted_project_shard_does_not_crash_show() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Corrupt one project's fields.json
    fs::write(
        cache_root(&dir).join("projects/DEMO/fields.json"),
        "not valid json {{{",
    )
    .unwrap();

    // cache show should still succeed (loads what it can)
    track_in_no_mock(&dir)
        .args(["-o", "json", "cache", "show"])
        .assert()
        .success();

    // OTHER project data should still be intact
    let output = track_in_no_mock(&dir)
        .args(["-o", "json", "cache", "show"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Both projects should still be listed (meta.json is fine)
    let projects = json["projects"].as_array().unwrap();
    assert!(projects.len() >= 2);
}

#[test]
fn test_corrupted_index_still_loads_shards() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Corrupt the index (but leave shard files intact)
    fs::write(cache_root(&dir).join("index.json"), "broken").unwrap();

    // Status should still work — ensure_* methods load shards independently.
    // The load() fails on index parse → unwrap_or_default → empty struct,
    // but ensure_projects/ensure_backend_shards read from shard files directly.
    let output = track_in_no_mock(&dir)
        .args(["-o", "json", "cache", "status"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // updated_at is null (came from index), but projects still load from shard dirs
    assert!(json["updated_at"].is_null());
    assert!(json["projects_count"].as_u64().unwrap() >= 2);
}

#[test]
fn test_missing_cache_directory_shows_empty() {
    let dir = temp_dir();

    // No cache exists at all
    track_in_no_mock(&dir)
        .args(["cache", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("empty"));
}

// =============================================================================
// Layout from before one directory per backend
// =============================================================================

/// A version 2 cache, which kept its shards directly in `.tracker-cache/`.
fn write_legacy_cache(dir: &Path) -> PathBuf {
    let root = dir.join(".tracker-cache");
    fs::create_dir_all(root.join("projects/DEMO")).unwrap();
    fs::write(
        root.join("index.json"),
        r#"{"version": 2, "updated_at": "2026-01-01T00:00:00Z",
            "backend_metadata": {"backend_type": "youtrack", "base_url": "https://mock.test"},
            "default_project": null}"#,
    )
    .unwrap();
    fs::write(
        root.join("projects/DEMO/meta.json"),
        r#"{"id": "0-1", "short_name": "DEMO", "name": "Demo", "description": null,
            "updated_at": "2026-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    root
}

#[test]
fn a_legacy_cache_moves_when_it_is_read_and_not_when_its_path_is_asked() {
    let dir = temp_dir();
    let root = write_legacy_cache(&dir);

    track_in_no_mock(&dir)
        .args(["cache", "path"])
        .assert()
        .success();
    assert!(
        root.join("index.json").is_file(),
        "`cache path` only reports where the cache is"
    );

    track_in_no_mock(&dir)
        .args(["cache", "status"])
        .assert()
        .success();
    assert!(!root.join("index.json").exists());
    assert!(root.join("youtrack/index.json").is_file());
    assert!(root.join("youtrack/projects/DEMO/meta.json").is_file());
}
