//! A repository's `./.track.toml` must not send your own token to a URL it
//! sets: track refuses such requests until `track config trust` allows them.
//!
//! Refusals happen before any request, so these tests need no server.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;

/// A port nothing listens on, for the cases that are allowed to connect.
const CLOSED_LOCAL_URL: &str = "http://127.0.0.1:9";

/// A fresh home directory (holding the global config) and a repository
/// checkout inside it.
struct Workspace {
    home: PathBuf,
    repo: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap().keep();
        let repo = home.join("cloned-repo");
        fs::create_dir_all(home.join(".tracker-cli")).unwrap();
        fs::create_dir_all(&repo).unwrap();
        Self { home, repo }
    }

    fn global_config(&self) -> PathBuf {
        self.home.join(".tracker-cli").join(".track.toml")
    }

    fn write_global(&self, toml: &str) -> &Self {
        fs::write(self.global_config(), toml).unwrap();
        self
    }

    fn write_repo(&self, toml: &str) -> &Self {
        fs::write(self.repo.join(".track.toml"), toml).unwrap();
        self
    }

    /// `track` run inside the repository with only this workspace's config.
    fn track(&self) -> assert_cmd::Command {
        let mut cmd = cargo_bin_cmd!("track");
        cmd.current_dir(&self.repo)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home);
        for var in [
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
            "LINEAR_TOKEN",
            "LINEAR_API_URL",
            "LINEAR_URL",
            "LINEAR_DEFAULT_TEAM",
            "TRACK_MOCK_DIR",
        ] {
            cmd.env_remove(var);
        }
        cmd
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let output = self
            .track()
            .args(["-o", "json"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "track {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

const GLOBAL_YOUTRACK: &str = r#"
backend = "youtrack"
[youtrack]
url = "https://victim.youtrack.cloud"
token = "perm:global-secret"
"#;

fn assert_refused(workspace: &Workspace, args: &[&str], backend: &str, endpoint: &str) {
    workspace
        .track()
        .args(args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "Refusing to send your {backend} token to {endpoint}"
        )))
        .stderr(predicate::str::contains("track config trust"))
        .stderr(predicate::str::contains("perm:global-secret").not());
}

fn assert_nothing_to_trust(workspace: &Workspace, extra_args: &[&str]) {
    let mut args = extra_args.to_vec();
    args.extend(["config", "trust"]);
    assert_eq!(workspace.json(&args)["trusted"], false, "args: {args:?}");
}

#[test]
fn refuses_repo_url_for_global_token() {
    let workspace = Workspace::new();
    workspace
        .write_global(GLOBAL_YOUTRACK)
        .write_repo("[youtrack]\nurl = \"https://attacker.example\"\n");

    assert_refused(
        &workspace,
        &["issue", "get", "DEMO-1"],
        "youtrack",
        "https://attacker.example",
    );
}

#[test]
fn refuses_repo_url_for_token_from_environment() {
    let workspace = Workspace::new();
    workspace.write_repo("[youtrack]\nurl = \"https://attacker.example\"\n");

    workspace
        .track()
        .env("YOUTRACK_TOKEN", "perm:env-secret")
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Refusing to send your youtrack token to https://attacker.example",
        ))
        .stderr(predicate::str::contains(
            "your own configuration sets no URL",
        ));
}

#[test]
fn refuses_repo_switching_backend_to_a_stored_token() {
    let workspace = Workspace::new();
    workspace
        .write_global(
            r#"
backend = "youtrack"
[jira]
url = "https://victim.atlassian.net"
email = "me@victim.example"
token = "jira-secret"
"#,
        )
        .write_repo("backend = \"jira\"\n[jira]\nurl = \"https://attacker.example\"\n");

    assert_refused(
        &workspace,
        &["issue", "get", "ABC-1"],
        "jira",
        "https://attacker.example",
    );
}

#[test]
fn refuses_repo_github_api_url() {
    let workspace = Workspace::new();
    workspace
        .write_global("[github]\ntoken = \"ghp_secret\"\nowner = \"o\"\nrepo = \"r\"\n")
        .write_repo("[github]\napi_url = \"https://attacker.example/api/v3\"\n");

    assert_refused(
        &workspace,
        &["-b", "github", "issue", "get", "1"],
        "github",
        "https://attacker.example",
    );
}

#[test]
fn refuses_repo_linear_api_url() {
    let workspace = Workspace::new();
    workspace
        .write_global("[linear]\ntoken = \"lin_secret\"\ndefault_team = \"ENG\"\n")
        .write_repo("[linear]\napi_url = \"https://attacker.example/graphql\"\n");

    assert_refused(
        &workspace,
        &["-b", "linear", "issue", "get", "ENG-1"],
        "linear",
        "https://attacker.example",
    );
}

#[test]
fn allows_what_does_not_move_your_token() {
    let workspace = Workspace::new();
    workspace.write_global(GLOBAL_YOUTRACK);

    // The repository supplies its own token.
    workspace.write_repo("[youtrack]\nurl = \"https://other.example\"\ntoken = \"repo-own\"\n");
    assert_nothing_to_trust(&workspace, &[]);

    // Same host, spelled differently.
    workspace.write_repo("[youtrack]\nurl = \"HTTPS://Victim.YouTrack.cloud:443/youtrack/\"\n");
    assert_nothing_to_trust(&workspace, &[]);

    // Settings that do not touch the connection.
    workspace.write_repo("default_project = \"DEMO\"\n");
    assert_nothing_to_trust(&workspace, &[]);

    // The user chose the URL, or the config file, explicitly.
    workspace.write_repo("[youtrack]\nurl = \"https://attacker.example\"\n");
    assert_nothing_to_trust(&workspace, &["--url", "https://attacker.example"]);
    let global = workspace.global_config();
    assert_nothing_to_trust(&workspace, &["--config", global.to_str().unwrap()]);
}

#[test]
fn trust_allows_that_destination_until_the_url_changes() {
    let workspace = Workspace::new();
    workspace
        .write_global(GLOBAL_YOUTRACK)
        .write_repo(&format!("[youtrack]\nurl = \"{CLOSED_LOCAL_URL}\"\n"));

    let shown = workspace.json(&["config", "show"]);
    assert_eq!(shown["token_redirect"]["endpoint"], CLOSED_LOCAL_URL);
    assert_eq!(shown["token_redirect"]["trusted"], false);

    let trusted = workspace.json(&["config", "trust"]);
    assert_eq!(trusted["trusted"], true);
    assert_eq!(trusted["already_trusted"], false);
    assert_eq!(
        workspace.json(&["config", "show"])["token_redirect"]["trusted"],
        true
    );

    // Trusted: the request is attempted (and fails only to connect).
    workspace
        .track()
        .args(["issue", "get", "DEMO-1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Refusing").not());

    // A different URL is a different destination.
    workspace.write_repo("[youtrack]\nurl = \"https://attacker.example\"\n");
    assert_refused(
        &workspace,
        &["issue", "get", "DEMO-1"],
        "youtrack",
        "https://attacker.example",
    );

    workspace.write_repo(&format!("[youtrack]\nurl = \"{CLOSED_LOCAL_URL}\"\n"));
    assert_eq!(workspace.json(&["config", "untrust"])["removed"], 1);
    assert_refused(
        &workspace,
        &["issue", "get", "DEMO-1"],
        "youtrack",
        CLOSED_LOCAL_URL,
    );
}

#[test]
fn doctor_reports_the_refusal_without_connecting() {
    let workspace = Workspace::new();
    workspace
        .write_global(GLOBAL_YOUTRACK)
        .write_repo("[youtrack]\nurl = \"https://attacker.example\"\n");

    let output = workspace
        .track()
        .args(["-o", "json", "doctor"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let checks = report["backends"][0]["checks"].as_array().unwrap();
    let config_valid = checks.iter().find(|c| c["name"] == "config_valid").unwrap();
    assert_eq!(config_valid["status"], "failed");
    assert!(
        config_valid["message"]
            .as_str()
            .unwrap()
            .contains("Refusing to send your youtrack token")
    );
    assert!(
        checks
            .iter()
            .filter(|c| c["name"] != "config_valid" && c["name"] != "write_check")
            .all(|c| c["status"] == "skipped"),
        "no remote check may run: {checks:?}"
    );
}

#[test]
fn config_show_reads_only_the_explicit_config_file() {
    let workspace = Workspace::new();
    workspace
        .write_global(GLOBAL_YOUTRACK)
        .write_repo("[youtrack]\nurl = \"https://attacker.example\"\n");
    let explicit = workspace.repo.join("explicit.toml");
    fs::write(&explicit, "default_project = \"EXPLICIT\"\n").unwrap();

    let shown = workspace.json(&["--config", explicit.to_str().unwrap(), "config", "show"]);
    let entries = shown["config"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "only the explicit file: {entries:?}");
    assert_eq!(entries[0]["key"], "default_project");
    assert_eq!(entries[0]["source"], "config");
    assert!(shown["token_redirect"].is_null());
}
