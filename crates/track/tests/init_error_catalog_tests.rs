//! Failing CLI pins for the #323 / #354 rewrite-now init-error catalog.
//!
//! These tests assert the target copy (phase + resource + write-status + one
//! next action). They are expected to fail on current main until Optimizer
//! rewrites the messages. Do not lock in today's accidental wording.

use assert_cmd::cargo::cargo_bin_cmd;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FIXTURE_OWNER: &str = "acme";
const FIXTURE_REPO: &str = "widgets";
const FIXTURE_PROJECT: &str = "acme/widgets";
const DUMMY_CREDENTIAL: &str = "test-token";

fn create_temp_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "track-init-catalog-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn track_with_home(temp_home: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("track");
    cmd.current_dir(temp_home)
        .args(["--color", "never"])
        .env("HOME", temp_home)
        .env("USERPROFILE", temp_home)
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

fn start_mock_http(
    status: u16,
    reason: &str,
    extra_headers: &[(&str, &str)],
    body: &str,
) -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let status_line = format!("HTTP/1.1 {status} {reason}");
    let header_block = extra_headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let body = body.to_string();

    let handle = thread::spawn(move || {
        if let Some(mut stream) = listener.incoming().flatten().next() {
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{header_block}\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Write);
            let mut discard = [0; 1024];
            while let Ok(n) = stream.read(&mut discard) {
                if n == 0 {
                    break;
                }
            }
        }
    });

    (port, handle)
}

fn closed_localhost_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}")
}

fn local_config_path(dir: &Path) -> PathBuf {
    dir.join(".track.toml")
}

/// Path text as `track init` reports it via `current_dir()`.
/// macOS temp dirs are under `/var`, which `current_dir()` resolves to `/private/var`.
fn cli_path_display(path: &Path) -> String {
    path.canonicalize()
        .ok()
        .map(|canonical| {
            let text = canonical.display().to_string();
            text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
        })
        .unwrap_or_else(|| path.display().to_string())
}

fn write_local_config(dir: &Path, contents: &str) {
    std::fs::write(local_config_path(dir), contents).unwrap();
}

fn assert_no_config(dir: &Path) {
    let path = local_config_path(dir);
    assert!(
        !path.exists(),
        "failed init should not write config at {}",
        path.display()
    );
}

fn primary_error_line(stderr: &str) -> &str {
    stderr
        .lines()
        .find(|line| line.starts_with("Error: "))
        .unwrap_or_else(|| stderr.trim())
}

fn failed_stderr(output: &Output) -> String {
    failed_stderr_hiding(output, &[])
}

fn failed_stderr_hiding(output: &Output, extra_forbidden: &[&str]) -> String {
    assert!(
        !output.status.success(),
        "expected command failure, got success. stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    for fragment in std::iter::once(DUMMY_CREDENTIAL).chain(extra_forbidden.iter().copied()) {
        assert!(
            !stderr.contains(fragment),
            "stderr must not reveal dummy credential or userinfo"
        );
    }
    stderr
}

fn run_init(dir: &Path, url: &str) -> Output {
    track_with_home(dir)
        .args(["init", "--url", url, "--token", DUMMY_CREDENTIAL])
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap()
}

fn run_github_init(dir: &Path, url: &str, extra: &[&str]) -> Output {
    let mut cmd = track_with_home(dir);
    cmd.args([
        "init",
        "--backend",
        "github",
        "--url",
        url,
        "--token",
        DUMMY_CREDENTIAL,
    ]);
    cmd.args(extra);
    cmd.timeout(Duration::from_secs(10)).output().unwrap()
}

fn run_first_github_command(dir: &Path) -> Output {
    track_with_home(dir)
        .args(["-b", "github", "project", "list"])
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap()
}

#[test]
fn github_project_omitted_uses_catalog_copy() {
    let dir = create_temp_dir();
    let output = run_github_init(&dir, "https://api.github.com", &[]);
    let stderr = failed_stderr(&output);
    assert_eq!(
        primary_error_line(&stderr),
        concat!(
            "Error: GitHub setup requires --project ",
            "<owner>/<repo>",
            ". Example: --project OrekGames/track-cli. No config was written."
        )
    );
    assert_no_config(&dir);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn github_project_malformed_uses_catalog_copy() {
    let cases = ["noslash", "acme/widgets/extra"];
    for value in cases {
        let dir = create_temp_dir();
        let output = run_github_init(&dir, "https://api.github.com", &["--project", value]);
        let stderr = failed_stderr(&output);
        let expected = format!(
            "Error: Invalid GitHub project '{value}': expected exactly {}, for example OrekGames/track-cli. No config was written.",
            "<owner>/<repo>"
        );
        assert_eq!(primary_error_line(&stderr), expected, "value={value}");
        assert_no_config(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn invalid_url_uses_catalog_copy() {
    for url in ["http://example.com", "not-a-url"] {
        let dir = create_temp_dir();
        let output = run_init(&dir, url);
        let stderr = failed_stderr(&output);
        let expected = format!(
            "Error: Invalid --url '{url}': expected an absolute https:// URL with a host. Plain http:// is allowed only for localhost. GitHub.com uses https://api.github.com. No config was written."
        );
        assert_eq!(primary_error_line(&stderr), expected, "url={url}");
        assert_no_config(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn userinfo_url_is_rejected_without_echoing_authority() {
    let dir = create_temp_dir();
    let url = "https://user:leak@example.com";
    let output = run_init(&dir, url);
    let stderr = failed_stderr_hiding(&output, &["@", "user:leak", "leak", url]);
    let line = primary_error_line(&stderr);
    assert!(
        line.contains("Invalid --url") || line.to_ascii_lowercase().contains("userinfo"),
        "expected invalid-url / userinfo phase, got {line:?}"
    );
    assert!(
        stderr.contains("No config was written."),
        "expected write-status in stderr:\n{stderr}"
    );
    assert_no_config(&dir);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_already_exists_uses_catalog_copy_and_leaves_file_unchanged() {
    let dir = create_temp_dir();
    let config_path = local_config_path(&dir);
    let original = "backend = \"youtrack\"\nurl = \"https://existing.example\"\n";
    write_local_config(&dir, original);

    let output = run_init(&dir, "https://example.com");
    let stderr = failed_stderr(&output);
    let expected = format!(
        "Error: Initialization stopped: config already exists at '{}'. No files were changed. Update it with 'track config set' or remove it only if you intend to replace the configuration.",
        cli_path_display(&config_path)
    );
    assert_eq!(primary_error_line(&stderr), expected);
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        original,
        "existing config must be unchanged"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gitignore_update_failure_after_local_write_uses_catalog_copy() {
    let dir = create_temp_dir();
    let config_path = local_config_path(&dir);
    let gitignore_path = dir.join(".gitignore");
    std::fs::create_dir(&gitignore_path).unwrap();

    let output = run_init(&dir, "https://example.com");
    let stderr = failed_stderr(&output);
    let prefix = format!(
        "Error: Config was created at '{}', but '{}' could not be updated:",
        cli_path_display(&config_path),
        cli_path_display(&gitignore_path)
    );
    assert!(
        primary_error_line(&stderr).starts_with(&prefix),
        "expected prefix {prefix:?}, got {:?}",
        primary_error_line(&stderr)
    );
    assert!(
        stderr.contains("Add '.track.toml' and '.tracker-cache/' manually before committing."),
        "missing next-action copy in stderr:\n{stderr}"
    );
    assert!(
        config_path.exists(),
        "config must remain after gitignore update failure"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn github_probe_status_errors_use_catalog_copy() {
    struct Case {
        status: u16,
        reason: &'static str,
        headers: &'static [(&'static str, &'static str)],
        body: &'static str,
        exact: Option<String>,
        prefix: Option<String>,
    }

    let cases = [
        Case {
            status: 401,
            reason: "Unauthorized",
            headers: &[],
            body: r#"{"message":"Bad credentials"}"#,
            exact: Some(format!(
                "Error: GitHub authentication failed while validating '{FIXTURE_PROJECT}'. Check that the token is valid and can read this repository. No config was written."
            )),
            prefix: None,
        },
        Case {
            status: 403,
            reason: "Forbidden",
            headers: &[("x-ratelimit-remaining", "12")],
            body: r#"{"message":"Resource not accessible by integration"}"#,
            exact: None,
            prefix: Some(format!(
                "Error: GitHub denied access to '{FIXTURE_PROJECT}' (403). Confirm the token can access the repository and, if applicable, is authorized for the organization. No config was written."
            )),
        },
        Case {
            status: 404,
            reason: "Not Found",
            headers: &[],
            body: r#"{"message":"Not Found"}"#,
            exact: Some(format!(
                "Error: GitHub could not access repository '{FIXTURE_PROJECT}' (404). Verify the owner/repo spelling and that the token can see a private repository. No config was written."
            )),
            prefix: None,
        },
        Case {
            status: 403,
            reason: "Forbidden",
            headers: &[("x-ratelimit-remaining", "0")],
            body: r#"{"message":"API rate limit exceeded"}"#,
            exact: Some(format!(
                "Error: GitHub rate limiting prevented validation of '{FIXTURE_PROJECT}'. Wait for the limit to reset or use a token with available quota, then retry. No config was written."
            )),
            prefix: None,
        },
    ];

    for case in cases {
        let dir = create_temp_dir();
        let (port, _server) = start_mock_http(case.status, case.reason, case.headers, case.body);
        let url = format!("http://127.0.0.1:{port}");
        let output = run_github_init(&dir, &url, &["--project", FIXTURE_PROJECT]);
        let stderr = failed_stderr(&output);
        let line = primary_error_line(&stderr);
        if let Some(exact) = &case.exact {
            assert_eq!(line, exact, "status={} stderr:\n{stderr}", case.status);
        }
        if let Some(prefix) = &case.prefix {
            assert!(
                line.starts_with(prefix.as_str()),
                "status={} expected prefix {prefix:?}, got {line:?}\n{stderr}",
                case.status
            );
        }
        assert_no_config(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn github_probe_transport_uses_catalog_copy() {
    let dir = create_temp_dir();
    let url = closed_localhost_url();
    let output = run_github_init(&dir, &url, &["--project", FIXTURE_PROJECT]);
    let stderr = failed_stderr(&output);
    let prefix = format!(
        "Error: Could not reach the GitHub API at '{url}' while validating '{FIXTURE_PROJECT}':"
    );
    assert!(
        primary_error_line(&stderr).starts_with(&prefix),
        "expected prefix {prefix:?}, got {:?}\n{stderr}",
        primary_error_line(&stderr)
    );
    assert!(
        stderr.contains(
            "Check the API URL, network, proxy, and TLS setup, then retry. No config was written."
        ),
        "missing next-action copy in stderr:\n{stderr}"
    );
    assert_no_config(&dir);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn github_probe_invalid_success_body_is_parse_not_http() {
    let dir = create_temp_dir();
    let (port, _server) = start_mock_http(200, "OK", &[], "not-json");
    let url = format!("http://127.0.0.1:{port}");
    let output = run_github_init(&dir, &url, &["--project", FIXTURE_PROJECT]);
    let stderr = failed_stderr(&output);
    let line = primary_error_line(&stderr);
    assert!(
        !line.starts_with("Error: Could not reach the GitHub API"),
        "decode failure must not use transport copy, got {line:?}"
    );
    assert!(
        !stderr.contains("HTTP error"),
        "decode failure must not be wrapped as Http:\n{stderr}"
    );
    let prefix = format!(
        "Error: The API at '{url}' returned an invalid GitHub repository response. Verify the API base URL or proxy. No config was written."
    );
    assert!(
        line.starts_with(&prefix),
        "expected parse prefix {prefix:?}, got {line:?}\n{stderr}"
    );
    assert_no_config(&dir);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn first_github_command_missing_token_uses_catalog_copy() {
    let dir = create_temp_dir();
    write_local_config(
        &dir,
        &format!(
            "backend = \"github\"\n\n[github]\nowner = \"{FIXTURE_OWNER}\"\nrepo = \"{FIXTURE_REPO}\"\n"
        ),
    );
    let stderr = failed_stderr(&run_first_github_command(&dir));
    assert_eq!(
        primary_error_line(&stderr),
        concat!(
            "Error: GitHub token is not configured. Set it with 'track config set github.token ",
            "<TOKEN>",
            "', set GITHUB_TOKEN, or pass --token."
        )
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn first_github_command_missing_owner_or_repo_uses_catalog_copy() {
    let cases = ["owner", "repo"];
    for field in cases {
        let dir = create_temp_dir();
        let section = match field {
            "owner" => format!("token = \"{DUMMY_CREDENTIAL}\"\nrepo = \"{FIXTURE_REPO}\"\n"),
            "repo" => format!("token = \"{DUMMY_CREDENTIAL}\"\nowner = \"{FIXTURE_OWNER}\"\n"),
            _ => unreachable!(),
        };
        write_local_config(
            &dir,
            &format!("backend = \"github\"\n\n[github]\n{section}"),
        );
        let stderr = failed_stderr(&run_first_github_command(&dir));
        let expected = format!(
            "Error: GitHub repository is not fully configured: missing github.{field}. Set it with 'track config set github.{field} {}' or re-run 'track init --backend github --project owner/repo'.",
            "<VALUE>"
        );
        assert_eq!(primary_error_line(&stderr), expected, "missing {field}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn github_config_test_names_github() {
    let dir = create_temp_dir();
    write_local_config(
        &dir,
        &format!(
            "backend = \"github\"\n\n[github]\nowner = \"{FIXTURE_OWNER}\"\nrepo = \"{FIXTURE_REPO}\"\n"
        ),
    );
    let (port, _server) =
        start_mock_http(401, "Unauthorized", &[], r#"{"message":"Bad credentials"}"#);
    let url = format!("http://127.0.0.1:{port}");
    let output = track_with_home(&dir)
        .args([
            "-b",
            "github",
            "--url",
            &url,
            "--token",
            DUMMY_CREDENTIAL,
            "config",
            "test",
        ])
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap();
    let stderr = failed_stderr(&output);
    assert!(
        primary_error_line(&stderr)
            .starts_with("Error: GitHub connection test failed while listing repositories:"),
        "expected GitHub-named prefix, got {:?}\n{stderr}",
        primary_error_line(&stderr)
    );
    assert!(
        stderr.contains(
            "Check github.token/GITHUB_TOKEN and run 'track -b github doctor' for per-check details."
        ),
        "missing next-action copy in stderr:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
