//! Tool definitions for the agent harness
//!
//! Defines the track CLI tool that agents can use.

use crate::anthropic::Tool;
use serde_json::json;

/// Create the track CLI tool definition
pub fn track_cli_tool() -> Tool {
    Tool {
        name: "track".to_string(),
        description: r#"Execute a track CLI command to interact with the issue tracker.

The track CLI supports the following commands:
- issue get <ID> - Get an issue by ID (e.g., track issue get DEMO-1)
- issue search <query> - Search for issues
- issue create -p <project> -s <summary> [-d <description>] [--state <state>] [--priority <priority>]
- issue update <ID> [--summary <summary>] [--state <state>] [--priority <priority>]
- issue comment <ID> -m <message> - Add a comment to an issue
- issue comments <ID> - List comments on an issue
- issue link <source> <target> [-t <type>] - Link two issues
- project list - List all projects
- project get <ID> - Get project details
- project fields <ID> - List custom fields for a project
- tags list - List all tags
- cache show - Show cached context

Use -o json for machine-readable output when you need to parse results.
Common issue states: Open, In Progress, Done
Common priorities: Low, Normal, High, Critical"#
            .to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Command line arguments to pass to track (e.g., [\"issue\", \"get\", \"DEMO-1\"] or [\"issue\", \"comment\", \"DEMO-1\", \"-m\", \"Working on this\"])"
                }
            },
            "required": ["args"]
        }),
    }
}

/// Parse track tool input
pub fn parse_track_input(input: &serde_json::Value) -> Result<Vec<String>, String> {
    let args = input
        .get("args")
        .ok_or("Missing 'args' field")?
        .as_array()
        .ok_or("'args' must be an array")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(String::from)
                .ok_or("All args must be strings")
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(args)
}

/// Execute a track command and return the result
pub fn execute_track(args: &[String], scenario_dir: &std::path::Path) -> (String, bool) {
    use std::process::Command;

    // Find the track binary
    let track_bin = find_track_binary();

    let result = Command::new(&track_bin)
        .args(args)
        .env("TRACK_MOCK_DIR", scenario_dir)
        .output();

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                (stdout.to_string(), false)
            } else {
                // Return stderr for errors
                let error_msg = if stderr.is_empty() {
                    stdout.to_string()
                } else {
                    stderr.to_string()
                };
                (error_msg, true)
            }
        }
        Err(e) => (format!("Failed to execute track: {}", e), true),
    }
}

/// Find the track binary
fn find_track_binary() -> String {
    // Check for debug build first
    let debug_path = "./target/debug/track";
    if std::path::Path::new(debug_path).exists() {
        return debug_path.to_string();
    }

    // Check for release build
    let release_path = "./target/release/track";
    if std::path::Path::new(release_path).exists() {
        return release_path.to_string();
    }

    // Fall back to PATH
    "track".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_track_input() {
        let input = json!({
            "args": ["issue", "get", "DEMO-1"]
        });

        let args = parse_track_input(&input).unwrap();
        assert_eq!(args, vec!["issue", "get", "DEMO-1"]);
    }

    #[test]
    fn test_parse_track_input_with_flags() {
        let input = json!({
            "args": ["issue", "comment", "DEMO-1", "-m", "Hello world"]
        });

        let args = parse_track_input(&input).unwrap();
        assert_eq!(
            args,
            vec!["issue", "comment", "DEMO-1", "-m", "Hello world"]
        );
    }
}
