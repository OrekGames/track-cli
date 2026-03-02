use anyhow::Result;

use crate::cache;
use crate::cli;
use crate::config::Config;
use crate::output;
use tracker_core::IssueTracker;

pub fn handle_open(id: Option<&str>, config: &Config, format: cli::OutputFormat) -> Result<()> {
    use colored::Colorize;

    let base_url = config
        .url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No URL configured"))?;

    // Remove trailing slash from base URL if present
    let base_url = base_url.trim_end_matches('/');

    let url = if let Some(issue_id) = id {
        // Open specific issue: https://youtrack.example.com/issue/PROJ-123
        format!("{}/issue/{}", base_url, issue_id)
    } else {
        // Open dashboard
        base_url.to_string()
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
pub fn is_issue_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 2
        && !parts[0].is_empty()
        && parts[0].chars().all(|c| c.is_ascii_alphanumeric())
        && !parts[1].is_empty()
        && parts[1].chars().all(|c| c.is_ascii_digit())
}

pub fn handle_issue_shortcut(
    client: &dyn IssueTracker,
    args: &[String],
    format: cli::OutputFormat,
) -> Result<()> {
    // Check if the first argument looks like an issue ID (e.g., PROJ-123)
    if args.is_empty() {
        return Err(anyhow::anyhow!(
            "unrecognized subcommand. Run 'track --help' for usage."
        ));
    }

    let potential_id = &args[0];

    if !is_issue_id(potential_id) {
        return Err(anyhow::anyhow!(
            "unrecognized subcommand '{}'. Run 'track --help' for usage.",
            potential_id
        ));
    }

    // Check for --full flag in remaining args
    let full = args.iter().any(|a| a == "--full");

    // Treat as `track issue get <ID>`
    let issue = client
        .get_issue(potential_id)
        .map_err(|e| anyhow::anyhow!("Failed to fetch issue '{}': {}", potential_id, e))?;

    // Record access for LRU tracking (same as issue get command)
    if let Ok(mut c) = cache::TrackerCache::load(None) {
        let _ = c.ensure_runtime_shards();
        c.record_issue_access(&issue);
        let _ = c.save_runtime(None);
    }

    if !full {
        output::output_result(&issue, format)?;
        return Ok(());
    }

    // Fetch additional context for full view
    let links = client.get_issue_links(potential_id)?;
    let comments = client.get_comments(potential_id)?;

    match format {
        cli::OutputFormat::Json => {
            let full_issue = serde_json::json!({
                "issue": issue,
                "links": links,
                "comments": comments
            });
            output::output_json(&full_issue)?;
        }
        cli::OutputFormat::Text => {
            use colored::Colorize;
            output::output_result(&issue, format)?;

            if !links.is_empty() {
                println!("\n  {}:", "Links".dimmed());
                for link in &links {
                    let direction = link.direction.as_deref().unwrap_or("BOTH");
                    let description = match direction {
                        "INWARD" => link
                            .link_type
                            .target_to_source
                            .as_deref()
                            .unwrap_or(&link.link_type.name),
                        "OUTWARD" => link
                            .link_type
                            .source_to_target
                            .as_deref()
                            .unwrap_or(&link.link_type.name),
                        _ => &link.link_type.name,
                    };
                    for linked_issue in &link.issues {
                        let linked_id = linked_issue
                            .id_readable
                            .as_deref()
                            .unwrap_or(&linked_issue.id);
                        let linked_summary = linked_issue.summary.as_deref().unwrap_or("");
                        println!(
                            "    {} {} - {}",
                            description.dimmed(),
                            linked_id.cyan(),
                            linked_summary
                        );
                    }
                }
            }

            if !comments.is_empty() {
                let recent_comments: Vec<_> = comments.iter().rev().take(5).collect();
                println!(
                    "\n  {} ({} total):",
                    "Recent Comments".dimmed(),
                    comments.len()
                );
                for comment in recent_comments.iter().rev() {
                    let author = comment
                        .author
                        .as_ref()
                        .map(|a| a.login.as_str())
                        .unwrap_or("unknown");
                    let date = comment
                        .created
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default();
                    println!("\n    [{}] {} wrote:", date.dimmed(), author.cyan());
                    for line in comment.text.lines().take(3) {
                        println!("      {}", line);
                    }
                    if comment.text.lines().count() > 3 {
                        println!("      ...");
                    }
                }
            }
        }
    }

    Ok(())
}
