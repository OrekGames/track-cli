//! Helpers shared by the `track` integration tests.
//!
//! Every test that runs `track` builds its command from [`track`],
//! [`track_in`] or [`track_mock`], so all of them start from the same clean
//! environment, and [`isolate`] is the one place that says what a
//! developer's shell, a CI runner or an agent session must not leak into a
//! test. A test binary includes this module with `mod common;` and uses only
//! part of it.

#![allow(dead_code)]

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::ffi::OsStr;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::TempDir;

// =============================================================================
// Environment
// =============================================================================

/// Every variable `track` reads by name. Most also fall under
/// [`ISOLATED_PREFIXES`], but naming them removes one even when a test set it
/// on the command before isolating it.
const ISOLATED_VARS: &[&str] = &[
    "TRACKER_URL",
    "TRACKER_TOKEN",
    "TRACKER_BACKEND",
    "TRACKER_CONFIG",
    "YOUTRACK_URL",
    "YOUTRACK_TOKEN",
    "JIRA_URL",
    "JIRA_EMAIL",
    "JIRA_TOKEN",
    "GITHUB_TOKEN",
    "GITHUB_OWNER",
    "GITHUB_REPO",
    "GITHUB_API_URL",
    "GITLAB_TOKEN",
    "GITLAB_URL",
    "GITLAB_PROJECT_ID",
    "GITLAB_NAMESPACE",
    "LINEAR_TOKEN",
    "LINEAR_API_URL",
    "LINEAR_URL",
    "LINEAR_DEFAULT_TEAM",
    "LINEAR_DEFAULT_PROJECT",
    "TRACK_MOCK_DIR",
    "TRACK_CACHE_DIR",
    "TRACK_CACHE_REFRESH_CONCURRENCY",
    "TRACK_NO_CACHE",
    "TRACK_SCHEMA_TTL",
    "TRACK_MAX_RESULTS",
    "TRACK_OUTPUT",
    "TRACK_ERROR_FORMAT",
    // Run from a Claude Code session, `track prime --hook` would append to
    // that session's real env file.
    "CLAUDE_ENV_FILE",
    // Where `track agent install` finds Codex's and Orek's configuration.
    "CODEX_HOME",
    "OREK_HOME",
    "XDG_CONFIG_HOME",
];

/// Prefixes `track` reads in bulk. The configuration merges every
/// `TRACKER_*` variable and every variable with a backend's prefix (an
/// unknown one becomes a top-level key), and matches them without regard to
/// case, so no list of names could be complete. `TRACK_*` is track's own
/// namespace.
const ISOLATED_PREFIXES: &[&str] = &[
    "TRACK_",
    "TRACKER_",
    "YOUTRACK_",
    "JIRA_",
    "GITHUB_",
    "GITLAB_",
    "LINEAR_",
];

fn has_isolated_prefix(name: &OsStr) -> bool {
    name.to_str().is_some_and(|name| {
        let name = name.to_ascii_uppercase();
        ISOLATED_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
    })
}

/// Remove from `cmd` every variable that changes what `track` does: the
/// tracker settings and credentials, track's own `TRACK_*` settings, and the
/// agent-session variables in [`ISOLATED_VARS`]. A test that needs one sets
/// it on the command afterwards.
pub fn isolate(cmd: &mut Command) -> &mut Command {
    for name in ISOLATED_VARS {
        cmd.env_remove(name);
    }
    for (name, _) in std::env::vars_os() {
        if has_isolated_prefix(&name) {
            cmd.env_remove(name);
        }
    }
    cmd
}

/// `track` with nothing from the environment that changes what it does (see
/// [`isolate`]), and with a scratch home directory that this test binary's
/// commands share, so it never reads or writes the user's own config and
/// cache. A command that needs a config or cache of its own runs
/// [`track_in`] a directory of its own instead.
pub fn track() -> Command {
    let mut cmd = cargo_bin_cmd!("track");
    let home = scratch_home();
    isolate(&mut cmd)
        .env("HOME", &home)
        .env("USERPROFILE", &home);
    cmd
}

/// The home directory of [`track`]: one per test binary, inside the target
/// directory Cargo gives integration tests for their own files.
fn scratch_home() -> PathBuf {
    let home = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(env!("CARGO_CRATE_NAME"))
        .join("home");
    fs::create_dir_all(&home).unwrap();
    home
}

/// [`track`] run in `dir`, with `dir` as its home, so the only config and
/// cache it sees are the test's own.
pub fn track_in(dir: &Path) -> Command {
    let mut cmd = track();
    cmd.current_dir(dir)
        .env("HOME", dir)
        .env("USERPROFILE", dir);
    cmd
}

/// [`track_in`] answered by the mock backend replaying `scenario` (see
/// [`copy_scenario`]). The URL and token are placeholders: the config is
/// checked before the mock takes over.
pub fn track_mock(dir: &Path, scenario: &Path) -> Command {
    let mut cmd = track_in(dir);
    cmd.env("TRACK_MOCK_DIR", scenario).args([
        "--url",
        "https://mock.test",
        "--token",
        "mock-token",
    ]);
    cmd
}

// =============================================================================
// Temporary directories
// =============================================================================

/// A directory for one test. It is removed when dropped, so also when an
/// assertion fails, and it can be used wherever a path is.
pub struct TestDir(TempDir);

impl Deref for TestDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<OsStr> for TestDir {
    fn as_ref(&self) -> &OsStr {
        self.0.path().as_os_str()
    }
}

/// A new, empty [`TestDir`] in the system's temporary directory.
pub fn temp_dir() -> TestDir {
    temp_dir_in(std::env::temp_dir())
}

/// [`temp_dir`], under the canonical form of the system's temporary
/// directory. GitHub Actions' Windows runners name it in 8.3 form
/// (`C:\Users\RUNNER~1\...`), and a `--config` path in that form fails
/// `Path::exists()` in the child although the test just wrote the file.
pub fn canonical_temp_dir() -> TestDir {
    let root = std::env::temp_dir();
    temp_dir_in(fs::canonicalize(&root).unwrap_or(root))
}

fn temp_dir_in(root: PathBuf) -> TestDir {
    TestDir(
        tempfile::Builder::new()
            .prefix("track-test-")
            .tempdir_in(root)
            .unwrap(),
    )
}

// =============================================================================
// Mock scenarios
// =============================================================================

/// The workspace root, which holds `fixtures/` and the live tests' config.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The checked-in mock scenarios.
pub fn fixtures_path() -> PathBuf {
    workspace_root().join("fixtures").join("scenarios")
}

pub fn copy_dir_recursive(from: &Path, to: &Path) {
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

/// Copy the scenario `name` into `dir` with an empty call log, so the test
/// can change it and read back only its own calls. Returns the copy.
pub fn copy_scenario(dir: &Path, name: &str) -> PathBuf {
    let scenario = dir.join(name);
    copy_dir_recursive(&fixtures_path().join(name), &scenario);
    fs::write(scenario.join("call_log.jsonl"), "").unwrap();
    scenario
}

/// Every call the mock logged for `scenario` (method and arguments), oldest
/// first.
pub fn mock_calls(scenario: &Path) -> Vec<Value> {
    fs::read_to_string(scenario.join("call_log.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// The methods of [`mock_calls`].
pub fn mock_call_methods(scenario: &Path) -> Vec<String> {
    mock_calls(scenario)
        .iter()
        .map(|entry| entry["method"].as_str().unwrap().to_string())
        .collect()
}

// =============================================================================
// Output
// =============================================================================

pub fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

pub fn parse_json_stdout(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON ({e}): {stdout}"))
}

/// Run `cmd`, which must succeed, and return its stdout.
pub fn stdout_of(cmd: &mut Command) -> String {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
