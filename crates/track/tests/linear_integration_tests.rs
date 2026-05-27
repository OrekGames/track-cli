//! Ignored live integration tests for the Linear backend.
//!
//! Run with:
//!   cargo test --package track --test linear_integration_tests -- --ignored
//!
//! Prerequisites:
//!   - .track.toml in the project root contains a [linear] section with:
//!     [linear]
//!     token = "lin_api_..."
//!     url = "https://linear.app/<workspace>"
//!   - The token can access team ORE.
//!   - For mutating tests, the workspace has a Linear project named "Track CLI".

use anyhow::{Context, Result, bail, ensure};
use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const LINEAR_TEAM: &str = "ORE";
const LINEAR_PROJECT: &str = "Track CLI";

fn repo_config_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".track.toml")
}

fn config_exists() -> bool {
    repo_config_path().exists()
}

fn unique_summary(prefix: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!(
        "track-cli linear live test {prefix}-{}-{ts}",
        std::process::id()
    )
}

fn unique_slug(prefix: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("track-cli-linear-live-{prefix}-{}-{ts}", std::process::id())
}

fn add_linear_env(cmd: &mut assert_cmd::Command) {
    cmd.env("LINEAR_DEFAULT_TEAM", LINEAR_TEAM)
        .env("LINEAR_DEFAULT_PROJECT", LINEAR_PROJECT);
}

fn track_linear_json() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "linear", "-o", "json", "--config"])
        .arg(repo_config_path());
    add_linear_env(&mut cmd);
    cmd
}

fn track_linear() -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.args(["-b", "linear", "--config"])
        .arg(repo_config_path());
    add_linear_env(&mut cmd);
    cmd
}

fn run_success(mut cmd: assert_cmd::Command) -> Result<Vec<u8>> {
    let debug = format!("{cmd:?}");
    let output = cmd
        .output()
        .with_context(|| format!("failed to execute {debug}"))?;

    if !output.status.success() {
        bail!(
            "command failed: {debug}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(output.stdout)
}

fn run_json(cmd: assert_cmd::Command) -> Result<Value> {
    let stdout = run_success(cmd)?;
    serde_json::from_slice(&stdout)
        .with_context(|| format!("invalid JSON output: {}", String::from_utf8_lossy(&stdout)))
}

fn delete_issue_best_effort(id: &str) {
    let mut cmd = track_linear();
    cmd.args(["issue", "delete", id]);
    let _ = cmd.output();
}

fn delete_tag_best_effort(name: &str) {
    let mut cmd = track_linear();
    cmd.args(["tags", "delete", name]);
    let _ = cmd.output();
}

fn issue_id(value: &Value) -> Result<String> {
    value["id_readable"]
        .as_str()
        .map(str::to_string)
        .context("issue JSON did not contain id_readable")
}

fn create_live_issue(prefix: &str) -> Result<String> {
    let summary = unique_summary(prefix);
    let mut cmd = track_linear_json();
    cmd.args([
        "issue",
        "create",
        "-p",
        LINEAR_TEAM,
        "--summary",
        &summary,
        "--description",
        "Temporary issue created by track Linear live tests",
    ]);
    let created = run_json(cmd)?;
    issue_id(&created)
}

fn custom_field_value<'a>(issue: &'a Value, name: &str) -> Option<&'a Value> {
    issue["custom_fields"].as_array()?.iter().find_map(|field| {
        field.as_object()?.values().find(|value| {
            value["name"]
                .as_str()
                .is_some_and(|field_name| field_name.eq_ignore_ascii_case(name))
        })
    })
}

fn issue_has_tag(issue: &Value, tag_name: &str) -> bool {
    issue["tags"].as_array().is_some_and(|tags| {
        tags.iter()
            .any(|tag| tag["name"].as_str() == Some(tag_name))
    })
}

fn link_id_matching(full_issue: &Value, link_type: &str, other_issue: &str) -> Option<String> {
    full_issue["links"].as_array()?.iter().find_map(|link| {
        if link["link_type"]["id"].as_str()? != link_type {
            return None;
        }

        let has_other = link["issues"].as_array()?.iter().any(|issue| {
            issue["id_readable"].as_str() == Some(other_issue)
                || issue["id"].as_str() == Some(other_issue)
        });

        has_other.then(|| link["id"].as_str().unwrap_or_default().to_string())
    })
}

#[test]
#[ignore]
fn test_linear_config_test_command() -> Result<()> {
    if !config_exists() {
        return Ok(());
    }

    let mut cmd = track_linear();
    cmd.args(["config", "test"]);
    run_success(cmd)?;
    Ok(())
}

#[test]
#[ignore]
fn test_linear_project_list_get_and_fields() -> Result<()> {
    if !config_exists() {
        return Ok(());
    }

    let mut cmd = track_linear_json();
    cmd.args(["project", "list"]);
    let teams = run_json(cmd)?;
    ensure!(
        teams.as_array().unwrap().iter().any(|team| {
            team["short_name"]
                .as_str()
                .is_some_and(|key| key == LINEAR_TEAM)
        }),
        "team {LINEAR_TEAM} was not present in project list"
    );

    let mut cmd = track_linear_json();
    cmd.args(["project", "get", LINEAR_TEAM]);
    let team = run_json(cmd)?;
    ensure!(
        team["short_name"].as_str() == Some(LINEAR_TEAM),
        "project get did not resolve team key {LINEAR_TEAM}"
    );

    let mut cmd = track_linear_json();
    cmd.args(["project", "fields", LINEAR_TEAM]);
    let fields = run_json(cmd)?;
    let names = fields
        .as_array()
        .context("fields output was not an array")?
        .iter()
        .filter_map(|field| field["name"].as_str())
        .collect::<Vec<_>>();
    ensure!(names.contains(&"Status"), "Status field missing");
    ensure!(names.contains(&"Assignee"), "Assignee field missing");
    ensure!(names.contains(&"Priority"), "Priority field missing");
    ensure!(names.contains(&"Labels"), "Labels field missing");
    ensure!(names.contains(&"Project"), "Linear Project field missing");
    Ok(())
}

#[test]
#[ignore]
fn test_linear_issue_search_read_only() -> Result<()> {
    if !config_exists() {
        return Ok(());
    }

    let mut cmd = track_linear_json();
    cmd.args([
        "issue",
        "search",
        &format!("project: {LINEAR_TEAM} #Unresolved"),
        "--limit",
        "5",
    ]);
    let issues = run_json(cmd)?;
    ensure!(issues.is_array(), "issue search output was not an array");

    let mut cmd = track_linear_json();
    cmd.args([
        "issue",
        "search",
        &format!("project: {LINEAR_TEAM} #Unresolved"),
        "--limit",
        "1",
        "--skip",
        "1",
    ]);
    let skipped = run_json(cmd)?;
    ensure!(
        skipped.is_array(),
        "issue search with skip output was not an array"
    );
    Ok(())
}

#[test]
#[ignore]
fn test_linear_tag_lifecycle() -> Result<()> {
    if !config_exists() {
        return Ok(());
    }

    let label = unique_slug("label");
    let renamed = format!("{label}-renamed");
    let labels_to_cleanup = vec![label.clone(), renamed.clone()];

    let result = (|| -> Result<()> {
        let mut cmd = track_linear_json();
        cmd.args([
            "tags",
            "create",
            &label,
            "--tag-color",
            "#f2c94c",
            "--description",
            "Temporary label created by track Linear live tests",
        ]);
        let created = run_json(cmd)?;
        ensure!(created["name"].as_str() == Some(label.as_str()));

        let mut cmd = track_linear_json();
        cmd.args([
            "tags",
            "update",
            &label,
            "--new-name",
            &renamed,
            "--tag-color",
            "#2f80ed",
        ]);
        let updated = run_json(cmd)?;
        ensure!(updated["name"].as_str() == Some(renamed.as_str()));

        let mut cmd = track_linear_json();
        cmd.args(["tags", "list"]);
        let tags = run_json(cmd)?;
        ensure!(
            tags.as_array()
                .context("tag list output was not an array")?
                .iter()
                .any(|tag| tag["name"].as_str() == Some(renamed.as_str())),
            "renamed label was not present in tag list"
        );

        let mut cmd = track_linear();
        cmd.args(["tags", "delete", &renamed]);
        run_success(cmd)?;
        Ok(())
    })();

    for label in labels_to_cleanup {
        delete_tag_best_effort(&label);
    }

    result
}

#[test]
#[ignore]
fn test_linear_issue_lifecycle_fields_comments_states_and_delete() -> Result<()> {
    if !config_exists() {
        return Ok(());
    }

    let summary = unique_summary("issue");
    let updated_summary = format!("{summary} updated");
    let label = unique_slug("issue-label");
    let mut issues_to_cleanup = Vec::new();
    let mut labels_to_cleanup = vec![label.clone()];

    let result = (|| -> Result<()> {
        let mut cmd = track_linear_json();
        cmd.args(["tags", "create", &label, "--tag-color", "#27ae60"]);
        run_json(cmd)?;

        let mut cmd = track_linear_json();
        cmd.args([
            "issue",
            "create",
            "-p",
            LINEAR_TEAM,
            "--summary",
            &summary,
            "--description",
            "Temporary issue created by track live tests",
            "--priority",
            "Medium",
            "--tag",
            &label,
            "--validate",
        ]);
        let created = run_json(cmd)?;
        let id = issue_id(&created)?;
        issues_to_cleanup.push(id.clone());
        ensure!(created["summary"].as_str() == Some(summary.as_str()));
        ensure!(
            issue_has_tag(&created, &label),
            "created issue did not include label"
        );
        ensure!(
            custom_field_value(&created, "Priority").and_then(|field| field["value"].as_str())
                == Some("Medium"),
            "created issue did not include Medium priority"
        );
        ensure!(
            custom_field_value(&created, "Project").and_then(|field| field["value"].as_str())
                == Some(LINEAR_PROJECT),
            "created issue did not include default Linear project association"
        );

        let comment = "Linear live test comment";
        let mut cmd = track_linear_json();
        cmd.args(["issue", "comment", &id, "--message", comment]);
        let created_comment = run_json(cmd)?;
        ensure!(created_comment["text"].as_str() == Some(comment));

        let mut cmd = track_linear_json();
        cmd.args(["issue", "comments", &id, "--limit", "5"]);
        let comments = run_json(cmd)?;
        ensure!(
            comments
                .as_array()
                .context("comments output was not an array")?
                .iter()
                .any(|entry| entry["text"].as_str() == Some(comment)),
            "created comment was not returned by issue comments"
        );

        let mut cmd = track_linear_json();
        cmd.args([
            "issue",
            "update",
            &id,
            "--summary",
            &updated_summary,
            "--priority",
            "High",
            "--tag",
            &label,
        ]);
        let updated = run_json(cmd)?;
        ensure!(updated["summary"].as_str() == Some(updated_summary.as_str()));
        ensure!(
            custom_field_value(&updated, "Priority").and_then(|field| field["value"].as_str())
                == Some("High"),
            "updated issue did not include High priority"
        );

        let mut cmd = track_linear_json();
        cmd.args(["issue", "start", &id]);
        run_json(cmd)?;

        let mut cmd = track_linear_json();
        cmd.args(["issue", "done", &id]);
        run_json(cmd)?;

        let mut cmd = track_linear_json();
        cmd.args(["issue", "get", &id]);
        let done = run_json(cmd)?;
        ensure!(
            custom_field_value(&done, "Status").and_then(|field| field["is_resolved"].as_bool())
                == Some(true),
            "issue done did not move issue to a resolved Linear state"
        );

        Ok(())
    })();

    for issue in issues_to_cleanup.iter().rev() {
        delete_issue_best_effort(issue);
    }
    for label in labels_to_cleanup.drain(..).rev() {
        delete_tag_best_effort(&label);
    }

    result
}

#[test]
#[ignore]
fn test_linear_parent_relation_link_unlink_and_delete() -> Result<()> {
    if !config_exists() {
        return Ok(());
    }

    let mut issues_to_cleanup = Vec::new();

    let result = (|| -> Result<()> {
        let parent = create_live_issue("parent")?;
        issues_to_cleanup.push(parent.clone());
        let child = create_live_issue("child")?;
        issues_to_cleanup.push(child.clone());
        let related = create_live_issue("relation")?;
        issues_to_cleanup.push(related.clone());

        let mut cmd = track_linear_json();
        cmd.args(["issue", "link", &child, &parent, "--type", "subtask"]);
        run_json(cmd)?;

        let mut cmd = track_linear_json();
        cmd.args(["issue", "get", &child, "--full"]);
        let child_full = run_json(cmd)?;
        let parent_link = link_id_matching(&child_full, "parent", &parent)
            .context("child issue did not report parent link")?;

        let mut cmd = track_linear_json();
        cmd.args(["issue", "unlink", &child, &parent_link]);
        run_json(cmd)?;

        let mut cmd = track_linear_json();
        cmd.args(["issue", "get", &child, "--full"]);
        let child_unlinked = run_json(cmd)?;
        ensure!(
            link_id_matching(&child_unlinked, "parent", &parent).is_none(),
            "parent link was still present after unlink"
        );

        let mut cmd = track_linear_json();
        cmd.args(["issue", "link", &parent, &related, "--type", "relates"]);
        run_json(cmd)?;

        let mut cmd = track_linear_json();
        cmd.args(["issue", "get", &parent, "--full"]);
        let parent_full = run_json(cmd)?;
        let relation_link = link_id_matching(&parent_full, "related", &related)
            .context("source issue did not report related link")?;

        let mut cmd = track_linear_json();
        cmd.args(["issue", "unlink", &parent, &relation_link]);
        run_json(cmd)?;

        let mut cmd = track_linear_json();
        cmd.args(["issue", "get", &parent, "--full"]);
        let parent_unlinked = run_json(cmd)?;
        ensure!(
            link_id_matching(&parent_unlinked, "related", &related).is_none(),
            "relation link was still present after unlink"
        );

        Ok(())
    })();

    for issue in issues_to_cleanup.iter().rev() {
        delete_issue_best_effort(issue);
    }

    result
}
