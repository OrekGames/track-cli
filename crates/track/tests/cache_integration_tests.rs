//! Integration tests for the sharded cache system (v2)
//!
//! These tests exercise cache commands end-to-end using the mock backend.
//! Each test that writes cache files uses a unique temp directory to avoid
//! interference between tests.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;

/// Get the path to the fixtures directory
fn fixtures_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent() // crates/
        .unwrap()
        .parent() // workspace root
        .unwrap()
        .join("fixtures")
        .join("scenarios")
}

/// Get path to the cache-operations scenario
fn cache_scenario() -> PathBuf {
    fixtures_path().join("cache-operations")
}

/// Create a unique temp directory for cache isolation.
/// Uses an atomic counter to guarantee uniqueness across parallel test threads.
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
        "track-cache-test-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Build a track command pointed at a temp directory with mock backend enabled.
/// Passes dummy --url and --token since config validation runs before mock activation.
fn track_in(dir: &PathBuf) -> Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir)
        .env("TRACK_MOCK_DIR", cache_scenario().to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"]);
    cmd
}

/// Build a track command pointed at a temp directory WITHOUT mock (for reading cache only).
/// Uses a dummy config so validation doesn't require a real server.
fn track_in_no_mock(dir: &PathBuf) -> Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir)
        .args(["--url", "https://mock.test", "--token", "mock-token"]);
    cmd
}

/// Run `track cache refresh` in the given directory to populate cache for subsequent tests.
fn populate_cache(dir: &PathBuf) {
    track_in(dir).args(["cache", "refresh"]).assert().success();
}

// =============================================================================
// cache refresh
// =============================================================================

#[test]
fn test_cache_refresh_creates_sharded_directory_layout() {
    let dir = temp_dir();
    populate_cache(&dir);

    let cache_dir = dir.join(".tracker-cache");

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

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cache_refresh_index_contains_version_and_metadata() {
    let dir = temp_dir();
    populate_cache(&dir);

    let index: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join(".tracker-cache/index.json")).unwrap())
            .unwrap();

    assert_eq!(index["version"], 2);
    assert!(index["updated_at"].is_string());
    assert_eq!(index["backend_metadata"]["backend_type"], "youtrack");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cache_refresh_project_shard_content() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Check project meta
    let meta: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".tracker-cache/projects/DEMO/meta.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(meta["short_name"], "DEMO");
    assert_eq!(meta["name"], "Demo Project");
    assert_eq!(meta["id"], "0-1");

    // Check fields
    let fields: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".tracker-cache/projects/DEMO/fields.json")).unwrap(),
    )
    .unwrap();
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
    let users: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".tracker-cache/projects/DEMO/users.json")).unwrap(),
    )
    .unwrap();
    let users_arr = users.as_array().unwrap();
    assert_eq!(users_arr.len(), 2);
    assert_eq!(users_arr[0]["login"], "john.doe");

    // Check workflow
    let workflow: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".tracker-cache/projects/DEMO/workflow.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(workflow["project_short_name"], "DEMO");
    let state_fields = workflow["state_fields"].as_array().unwrap();
    assert!(
        !state_fields.is_empty(),
        "Should have state field workflows"
    );

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cache_refresh_backend_shard_content() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Tags
    let tags: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".tracker-cache/backend/tags.json")).unwrap(),
    )
    .unwrap();
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
    let link_types: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".tracker-cache/backend/link_types.json")).unwrap(),
    )
    .unwrap();
    assert!(
        !link_types.as_array().unwrap().is_empty(),
        "Should have link types"
    );

    // Query templates
    let templates: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".tracker-cache/backend/query_templates.json")).unwrap(),
    )
    .unwrap();
    let template_names: Vec<&str> = templates
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(template_names.contains(&"unresolved"));
    assert!(template_names.contains(&"bugs"));

    fs::remove_dir_all(&dir).unwrap();
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

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cache_refresh_json_output() {
    let dir = temp_dir();
    track_in(&dir)
        .args(["-o", "json", "cache", "refresh"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""success": true"#));

    fs::remove_dir_all(&dir).unwrap();
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

    fs::remove_dir_all(&dir).unwrap();
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

    fs::remove_dir_all(&dir).unwrap();
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

    fs::remove_dir_all(&dir).unwrap();
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

    fs::remove_dir_all(&dir).unwrap();
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

    fs::remove_dir_all(&dir).unwrap();
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
        path_str.ends_with(".tracker-cache"),
        "cache path should end with .tracker-cache, got: {}",
        path_str
    );

    fs::remove_dir_all(&dir).unwrap();
}

// =============================================================================
// cache refresh --if-stale
// =============================================================================

#[test]
fn test_cache_refresh_if_stale_skips_fresh_cache() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Cache was just created, 1d staleness threshold should skip
    track_in_no_mock(&dir)
        .args(["cache", "refresh", "--if-stale", "1d"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fresh"));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_cache_refresh_if_stale_json_output() {
    let dir = temp_dir();
    populate_cache(&dir);

    let output = track_in_no_mock(&dir)
        .args(["-o", "json", "cache", "refresh", "--if-stale", "1d"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(json["skipped"], true);
    assert!(json["age_seconds"].is_number());

    fs::remove_dir_all(&dir).unwrap();
}

// =============================================================================
// context command
// =============================================================================

#[test]
fn test_context_loads_from_cache() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Context should load from existing cache without needing mock
    track_in_no_mock(&dir)
        .args(["context"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DEMO"))
        .stdout(predicate::str::contains("Projects"));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_context_json_output_from_cache() {
    let dir = temp_dir();
    populate_cache(&dir);

    let output = track_in_no_mock(&dir)
        .args(["-o", "json", "context"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert!(json["projects"].as_array().unwrap().len() >= 2);
    assert!(!json["project_fields"].as_array().unwrap().is_empty());
    assert!(!json["tags"].as_array().unwrap().is_empty());
    assert!(!json["query_templates"].as_array().unwrap().is_empty());
    assert!(!json["workflow_hints"].as_array().unwrap().is_empty());

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_context_project_filter() {
    let dir = temp_dir();
    populate_cache(&dir);

    let output = track_in_no_mock(&dir)
        .args(["-o", "json", "context", "--project", "DEMO"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Only DEMO project should be present
    let projects = json["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["short_name"], "DEMO");

    fs::remove_dir_all(&dir).unwrap();
}

// =============================================================================
// Issue access recording (runtime shard isolation)
// =============================================================================

#[test]
fn test_issue_access_preserves_other_shards() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Read all shard files before issue access
    let tags_before = fs::read_to_string(dir.join(".tracker-cache/backend/tags.json")).unwrap();
    let fields_before =
        fs::read_to_string(dir.join(".tracker-cache/projects/DEMO/fields.json")).unwrap();
    let meta_before =
        fs::read_to_string(dir.join(".tracker-cache/projects/DEMO/meta.json")).unwrap();

    // Access an issue (triggers record_issue_access + save_runtime)
    let scenario = fixtures_path().join("basic-workflow");
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(&dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .args(["--url", "https://mock.test", "--token", "mock-token"])
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .success();

    // Verify other shards were NOT modified
    let tags_after = fs::read_to_string(dir.join(".tracker-cache/backend/tags.json")).unwrap();
    let fields_after =
        fs::read_to_string(dir.join(".tracker-cache/projects/DEMO/fields.json")).unwrap();
    let meta_after =
        fs::read_to_string(dir.join(".tracker-cache/projects/DEMO/meta.json")).unwrap();

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
    let recent: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".tracker-cache/runtime/recent_issues.json")).unwrap(),
    )
    .unwrap();
    let recent_arr = recent.as_array().unwrap();
    assert!(
        !recent_arr.is_empty(),
        "recent_issues should contain the accessed issue"
    );
    assert_eq!(recent_arr[0]["id_readable"], "DEMO-1");

    fs::remove_dir_all(&dir).unwrap();
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
        dir.join(".tracker-cache/projects/DEMO/fields.json"),
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

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_corrupted_index_still_loads_shards() {
    let dir = temp_dir();
    populate_cache(&dir);

    // Corrupt the index (but leave shard files intact)
    fs::write(dir.join(".tracker-cache/index.json"), "broken").unwrap();

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

    fs::remove_dir_all(&dir).unwrap();
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

    fs::remove_dir_all(&dir).unwrap();
}
