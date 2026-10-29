use anyhow::Result;

use crate::cli::{self, Backend};
use crate::config::Config;
use crate::output;
use tracker_core::IssueTracker;

/// Browser URL of `issue_id` on the tracker at `base_url`, or of the tracker
/// itself (dashboard) when no issue is given.
fn browser_url(backend: Backend, base_url: &str, issue_id: Option<&str>) -> String {
    let base_url = base_url.trim_end_matches('/');
    match issue_id {
        // Jira serves issue pages under /browse/; YouTrack and Linear under /issue/.
        Some(id) if backend == Backend::Jira => format!("{base_url}/browse/{id}"),
        Some(id) => format!("{base_url}/issue/{id}"),
        None => base_url.to_string(),
    }
}

pub fn handle_open(
    client: &dyn IssueTracker,
    id: Option<&str>,
    config: &Config,
    backend: Backend,
    format: cli::OutputFormat,
) -> Result<()> {
    use colored::Colorize;

    // GitHub and GitLab are configured with API URLs, so they build their own
    // browser URLs; the other backends serve their UI from the configured URL.
    let url = match client.web_url(id)? {
        Some(url) => url,
        None => {
            let base_url = config
                .url
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No URL configured"))?;
            browser_url(backend, base_url, id)
        }
    };

    // Try to open in browser
    let result = open::that(&url);

    match format {
        cli::OutputFormat::Json => {
            if result.is_ok() {
                output::output_json(&serde_json::json!({
                    "success": true,
                    "url": url
                }))?;
            } else {
                output::output_json(&serde_json::json!({
                    "success": false,
                    "url": url,
                    "error": "Failed to open browser"
                }))?;
            }
        }
        cli::OutputFormat::Text => {
            if result.is_ok() {
                if let Some(issue_id) = id {
                    println!("Opening {} in browser...", issue_id.cyan().bold());
                } else {
                    println!("Opening dashboard in browser...");
                }
            } else {
                // If we can't open the browser, at least print the URL
                println!("Could not open browser. URL: {}", url.cyan());
            }
        }
    }

    Ok(())
}

/// Check if a string looks like an issue ID (e.g., PROJ-123)
fn is_issue_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 2
        && !parts[0].is_empty()
        && parts[0].chars().all(|c| c.is_ascii_alphanumeric())
        && !parts[1].is_empty()
        && parts[1].chars().all(|c| c.is_ascii_digit())
}

/// The issue ID a `track PROJ-123 [--full]` shortcut names, or the error for
/// an unrecognized subcommand.
pub fn shortcut_issue_id(args: &[String]) -> Result<&str> {
    match args.first() {
        Some(id) if is_issue_id(id) => Ok(id),
        Some(cmd) => Err(anyhow::anyhow!(
            "unrecognized subcommand '{}'. Run 'track --help' for usage.",
            cmd
        )),
        None => Err(anyhow::anyhow!(
            "unrecognized subcommand. Run 'track --help' for usage."
        )),
    }
}

/// `track PROJ-123 [--full]` is shorthand for `track issue get PROJ-123 [--full]`.
pub fn handle_issue_shortcut(
    client: &dyn IssueTracker,
    args: &[String],
    format: cli::OutputFormat,
) -> Result<()> {
    let id = shortcut_issue_id(args)?;
    let full = args.iter().any(|a| a == "--full");
    super::issue::handle_get(client, id, full, format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_url_uses_backend_issue_path() {
        assert_eq!(
            browser_url(Backend::Jira, "https://acme.atlassian.net/", Some("PROJ-1")),
            "https://acme.atlassian.net/browse/PROJ-1"
        );
        assert_eq!(
            browser_url(Backend::YouTrack, "https://yt.example.com", Some("PROJ-1")),
            "https://yt.example.com/issue/PROJ-1"
        );
        assert_eq!(
            browser_url(Backend::Linear, "https://linear.app/acme/", Some("ENG-7")),
            "https://linear.app/acme/issue/ENG-7"
        );
        assert_eq!(
            browser_url(Backend::Jira, "https://acme.atlassian.net/", None),
            "https://acme.atlassian.net"
        );
    }

    #[test]
    fn is_issue_id_valid_standard() {
        assert!(is_issue_id("PROJ-123"));
        assert!(is_issue_id("A-1"));
        assert!(is_issue_id("DEMO-42"));
        assert!(is_issue_id("MY2PROJ-99"));
    }

    #[test]
    fn is_issue_id_valid_lowercase() {
        assert!(is_issue_id("proj-123"));
        assert!(is_issue_id("demo-1"));
    }

    #[test]
    fn is_issue_id_invalid_no_dash() {
        assert!(!is_issue_id("PROJ123"));
        assert!(!is_issue_id("abc"));
        assert!(!is_issue_id("123"));
    }

    #[test]
    fn is_issue_id_invalid_empty_parts() {
        assert!(!is_issue_id("-123"));
        assert!(!is_issue_id("PROJ-"));
        assert!(!is_issue_id("-"));
    }

    #[test]
    fn is_issue_id_invalid_multiple_dashes() {
        assert!(!is_issue_id("PROJ-SUB-123"));
        assert!(!is_issue_id("A-B-1"));
    }

    #[test]
    fn is_issue_id_invalid_non_digit_suffix() {
        assert!(!is_issue_id("PROJ-abc"));
        assert!(!is_issue_id("PROJ-12a"));
    }

    #[test]
    fn is_issue_id_invalid_special_chars_prefix() {
        assert!(!is_issue_id("PR@J-123"));
        assert!(!is_issue_id("PR OJ-123"));
    }

    #[test]
    fn is_issue_id_empty_string() {
        assert!(!is_issue_id(""));
    }
}
