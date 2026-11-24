//! `-o agent`: compact text that never drops data without saying so, plus
//! the output changes that keep JSON honest and help short.

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
        "track-agent-out-test-{}-{}-{}",
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

fn copy_scenario(dir: &Path, name: &str) -> PathBuf {
    let scenario = dir.join(name);
    copy_dir_recursive(&fixtures_path().join(name), &scenario);
    fs::write(scenario.join("call_log.jsonl"), "").unwrap();
    scenario
}

fn track_mock(dir: &Path, scenario: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(dir)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env("TRACK_MOCK_DIR", scenario.to_str().unwrap())
        .env("TRACKER_DEFAULT_PROJECT", "DEMO")
        .env_remove("TRACK_OUTPUT")
        .args(["--url", "https://mock.test", "--token", "mock-token"]);
    cmd
}

fn stdout_of(cmd: &mut assert_cmd::Command) -> String {
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

const SEARCH: [&str; 3] = ["issue", "search", "project: DEMO #Unresolved"];

#[test]
fn agent_search_is_one_line_per_issue_with_a_verdict_on_completeness() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-search-templates");

    let stdout = stdout_of(
        track_mock(&dir, &scenario)
            .args(["-o", "agent"])
            .args(SEARCH),
    );
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        [
            "# id\tstate\tpriority\tassignee\tsummary",
            "DEMO-1\t-\t-\t-\tImplement user authentication",
            "DEMO-2\t-\t-\t-\tFix login timeout",
            "# 2 shown, complete",
        ]
    );

    // A full page is not presented as the whole result.
    let stdout = stdout_of(
        track_mock(&dir, &scenario)
            .args(["-o", "agent"])
            .args(SEARCH)
            .args(["--limit", "2"]),
    );
    assert_eq!(
        stdout.lines().last(),
        Some("# 2 shown; more may exist: --skip 2, or --all for everything")
    );

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn agent_get_full_carries_everything_in_fewer_bytes() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-get-full");
    let full = ["issue", "get", "DEMO-1", "--full"];

    let agent = stdout_of(track_mock(&dir, &scenario).args(["-o", "agent"]).args(full));
    assert_eq!(
        agent.lines().collect::<Vec<_>>(),
        [
            "DEMO-1  Implement user authentication",
            "State: Open | Priority: Normal",
            "project: DEMO | tags: backend | created: 2024-01-10 | updated: 2024-01-15",
            "description:",
            "Add login/logout functionality",
            "links (1):",
            "relates to DEMO-2  Related issue  (link id link-1)",
            "comments (1):",
            "[2024-01-12 10:00] john.doe: First comment",
        ]
    );

    let json = stdout_of(track_mock(&dir, &scenario).args(["-o", "json"]).args(full));
    assert!(
        agent.len() * 3 < json.len(),
        "agent {} bytes, json {} bytes",
        agent.len(),
        json.len()
    );
    assert!(!agent.contains('\u{1b}'), "no colour codes in agent output");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn agent_get_full_names_the_comments_it_leaves_out() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-get-full");
    // 25 comments, each longer than the terminal view would show.
    let comments: Vec<serde_json::Value> = (1..=25)
        .map(|n| {
            serde_json::json!({
                "id": format!("c{n}"),
                "text": format!("comment {n} line 1\nline 2\nline 3\nline 4\nline 5"),
                "author": {"login": "john.doe", "name": "John Doe"},
                "created": "2024-01-12T10:00:00Z"
            })
        })
        .collect();
    for entry in fs::read_dir(scenario.join("responses")).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().contains("comments"))
        {
            fs::write(&path, serde_json::to_string(&comments).unwrap()).unwrap();
        }
    }

    let agent = stdout_of(
        track_mock(&dir, &scenario)
            .args(["-o", "agent"])
            .args(["issue", "get", "DEMO-1", "--full"]),
    );

    assert!(
        agent.contains("comments (last 20 of 25; all: track i comments DEMO-1 --all):"),
        "{agent}"
    );
    assert!(!agent.contains("comment 5 line 1"));
    assert!(agent.contains("comment 6 line 1"));
    // Bodies are whole, not cut to three lines.
    assert!(agent.contains("comment 25 line 1\nline 2\nline 3\nline 4\nline 5"));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn agent_writes_are_acknowledged_in_one_line() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "cache-efficiency");

    let stdout = stdout_of(track_mock(&dir, &scenario).args([
        "-o",
        "agent",
        "issue",
        "create",
        "-s",
        "Fix login timeout bug",
        "--field",
        "Priority=Normal",
        "--field",
        "Type=Bug",
    ]));

    assert_eq!(
        stdout,
        "created DEMO-10  Fix login timeout bug | State: Open | Priority: Normal | Type: Bug\n"
    );

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn agent_project_fields_include_the_values() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-get-full");

    let stdout =
        stdout_of(track_mock(&dir, &scenario).args(["-o", "agent", "project", "fields", "DEMO"]));

    // The terminal view lists names and types only.
    assert!(stdout.contains("State (state, required): Open, In Progress, Done*"));
    assert!(stdout.contains("Priority (enum, required): Low, Normal, High, Critical"));
    assert!(!stdout.contains("\n\n"));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn track_output_env_sets_the_default_and_the_flag_wins() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-search-templates");

    let from_env = stdout_of(
        track_mock(&dir, &scenario)
            .env("TRACK_OUTPUT", "agent")
            .args(SEARCH),
    );
    assert!(from_env.starts_with("# id\tstate"), "{from_env}");

    let flag_wins = stdout_of(
        track_mock(&dir, &scenario)
            .env("TRACK_OUTPUT", "agent")
            .args(["-o", "json"])
            .args(SEARCH),
    );
    assert!(flag_wins.trim_start().starts_with('['), "{flag_wins}");

    // Empty or unknown values mean "not set", not an error.
    for value in ["", "nonsense"] {
        let text = stdout_of(
            track_mock(&dir, &scenario)
                .env("TRACK_OUTPUT", value)
                .args(SEARCH),
        );
        assert!(
            text.starts_with("DEMO-1 - "),
            "TRACK_OUTPUT={value:?}: {text}"
        );
    }

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn select_is_refused_for_text_output_including_agent() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-search-templates");

    for format in ["text", "agent"] {
        track_mock(&dir, &scenario)
            .args(["-o", format, "--select", "id"])
            .args(SEARCH)
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("--select needs JSON output"));
    }

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn transition_json_reports_what_the_tracker_did() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "issue-start-complete");

    // The fixture answers every update of DEMO-3 with a "Done" issue.
    let output = track_mock(&dir, &scenario)
        .args(["-o", "json", "issue", "complete", "DEMO-3"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(json["id_readable"], json["idReadable"]);
    assert_eq!(json["state"], "Done");
    assert_eq!(json["verified"], json["actual_state"] == "Done");
    assert!(json["warnings"].is_array());
    assert_eq!(
        json["verified"] == false,
        !json["warnings"].as_array().unwrap().is_empty(),
        "an unverified transition must say why: {json}"
    );

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn subcommand_help_lists_its_own_options_only() {
    let dir = temp_dir();

    let help = stdout_of(
        cargo_bin_cmd!("track")
            .current_dir(&dir)
            .args(["cache", "-h"]),
    );
    assert!(help.contains("refresh"));
    assert!(!help.contains("--token <TOKEN>"), "{help}");
    assert!(help.contains("Global options (-o, -b,"));
    assert!(help.len() < 700, "cache -h is {} bytes", help.len());

    // They are still documented once, and still accepted everywhere.
    let top = stdout_of(cargo_bin_cmd!("track").current_dir(&dir).arg("--help"));
    for flag in ["--format", "--backend", "--select", "--no-cache", "--token"] {
        assert!(top.contains(flag), "top-level help must list {flag}");
    }
    cargo_bin_cmd!("track")
        .current_dir(&dir)
        .env("HOME", &dir)
        .env("USERPROFILE", &dir)
        .args(["workflow", "list", "-o", "json", "-b", "github"])
        .assert()
        .success();

    // `init` keeps its own --url and --token, visible in its help.
    let init = stdout_of(
        cargo_bin_cmd!("track")
            .current_dir(&dir)
            .args(["init", "-h"]),
    );
    assert!(init.contains("--url") && init.contains("--token"), "{init}");

    fs::remove_dir_all(&dir).unwrap();
}
