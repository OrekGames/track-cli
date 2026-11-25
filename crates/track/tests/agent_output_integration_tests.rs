//! `-o agent`: compact text that never drops data without saying so, plus
//! the output changes that keep JSON honest and help short.

use predicates::prelude::*;
use std::fs;
use std::path::Path;

mod common;
use common::{copy_scenario, mock_call_methods, stdout_of, temp_dir, track_in};

/// [`common::track_mock`] with DEMO as the default project.
fn track_mock(dir: &Path, scenario: &Path) -> assert_cmd::Command {
    let mut cmd = common::track_mock(dir, scenario);
    cmd.env("TRACKER_DEFAULT_PROJECT", "DEMO");
    cmd
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
}

#[test]
fn agent_article_lists_are_one_line_each_with_a_verdict_on_completeness() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "article-workflow");

    let stdout = stdout_of(
        track_mock(&dir, &scenario).args(["-o", "agent", "article", "list", "-p", "DEMO"]),
    );
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        [
            "KB-A-1  Getting Started | project: DEMO | created: 2024-01-01 | updated: 2024-01-15",
            "KB-A-2  API Reference | project: DEMO | created: 2024-01-05 | updated: 2024-01-10",
            "# 2 shown, complete",
        ]
    );

    // A full page says how to get the rest, on stdout, like issue search.
    for list in [
        &["article", "list", "-p", "DEMO", "--limit", "2"][..],
        &["article", "search", "docs", "--limit", "1"],
    ] {
        let stdout = stdout_of(track_mock(&dir, &scenario).args(["-o", "agent"]).args(list));
        let skip = list.last().unwrap();
        assert_eq!(
            stdout.lines().last(),
            Some(
                format!("# {skip} shown; more may exist: --skip {skip}, or --all for everything")
                    .as_str()
            ),
            "{list:?}: {stdout}"
        );
        assert!(
            !stdout.contains("documentation"),
            "no article content in a list: {stdout}"
        );
    }
}

#[test]
fn a_select_that_misses_after_a_write_does_not_fail_the_write() {
    let dir = temp_dir();

    // `idReadable` is not a key of the JSON: the issue is created anyway, so
    // the command must not exit as if it failed and invite a second create.
    let scenario = copy_scenario(&dir, "cache-efficiency");
    let output = track_mock(&dir, &scenario)
        .args(["-o", "json", "--select", "idReadable"])
        .args(["issue", "create", "-s", "Fix login timeout bug"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--select matched nothing"), "{stderr}");
    // The whole result, so the new issue's id is not lost.
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["id_readable"], "DEMO-10", "{json}");
    assert_eq!(
        mock_call_methods(&scenario)
            .iter()
            .filter(|method| *method == "create_issue")
            .count(),
        1
    );

    let scenario = copy_scenario(&dir, "basic-workflow");
    let output = track_mock(&dir, &scenario)
        .args(["-o", "json", "--select", "idReadable"])
        .args(["issue", "update", "DEMO-1", "--summary", "x"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["id_readable"], "DEMO-1", "{json}");

    // A read that selects nothing still fails: nothing was done.
    track_mock(&dir, &scenario)
        .args([
            "-o",
            "json",
            "--select",
            "idReadable",
            "issue",
            "get",
            "DEMO-1",
        ])
        .assert()
        .code(5)
        .stdout(predicate::str::is_empty());
}

#[test]
fn selecting_a_field_the_issue_has_without_a_value_gives_null() {
    let dir = temp_dir();
    let scenario = copy_scenario(&dir, "basic-workflow");
    // Unassign DEMO-1.
    let path = scenario.join("responses").join("get_issue_DEMO-1.json");
    let mut issue: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    for field in issue["custom_fields"].as_array_mut().unwrap() {
        if let Some(user) = field.get_mut("SingleUser") {
            user["login"] = serde_json::Value::Null;
            user["display_name"] = serde_json::Value::Null;
        }
    }
    fs::write(&path, issue.to_string()).unwrap();

    let get = ["-o", "json", "issue", "get", "DEMO-1", "--select"];
    let json: serde_json::Value = serde_json::from_str(&stdout_of(
        track_mock(&dir, &scenario).args(get).arg("fields.Assignee"),
    ))
    .unwrap();
    assert_eq!(json, serde_json::json!({"fields": {"Assignee": null}}));

    // A field the issue does not have is still a miss.
    track_mock(&dir, &scenario)
        .args(get)
        .arg("fields.Asignee")
        .assert()
        .code(5);
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

    assert_eq!(json["id_readable"], "DEMO-3");
    assert!(
        json.get("idReadable").is_none(),
        "the camelCase key is gone in 2.0"
    );
    assert_eq!(json["state"], "Done");
    assert_eq!(json["verified"], json["actual_state"] == "Done");
    assert!(json["warnings"].is_array());
    assert_eq!(
        json["verified"] == false,
        !json["warnings"].as_array().unwrap().is_empty(),
        "an unverified transition must say why: {json}"
    );
}

#[test]
fn subcommand_help_lists_its_own_options_only() {
    let dir = temp_dir();

    let help = stdout_of(track_in(&dir).args(["cache", "-h"]));
    assert!(help.contains("refresh"));
    assert!(!help.contains("--token <TOKEN>"), "{help}");
    // Generated from the global options, so it names every one of them.
    assert!(
        help.contains(
            "Global options (-o, --select, -v, --color, -b, --config, --url, --token, --no-cache, --error-format)"
        ),
        "{help}"
    );
    assert!(help.len() < 700, "cache -h is {} bytes", help.len());

    // They are still documented once, and still accepted everywhere.
    let top = stdout_of(track_in(&dir).arg("--help"));
    for flag in ["--format", "--backend", "--select", "--no-cache", "--token"] {
        assert!(top.contains(flag), "top-level help must list {flag}");
    }
    track_in(&dir)
        .args(["workflow", "list", "-o", "json", "-b", "github"])
        .assert()
        .success();

    // `init` keeps its own --url and --token, visible in its help.
    let init = stdout_of(track_in(&dir).args(["init", "-h"]));
    assert!(init.contains("--url") && init.contains("--token"), "{init}");
}
