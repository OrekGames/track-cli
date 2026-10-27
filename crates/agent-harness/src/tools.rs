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

/// Find the track binary: a local debug build, then a local release build, then
/// `track` on PATH.
///
/// Local builds are canonicalized because agent CLIs are given this path (and
/// its directory on PATH) and may run commands from another working directory.
pub(crate) fn find_track_binary() -> String {
    for local_build in ["./target/debug/track", "./target/release/track"] {
        if std::path::Path::new(local_build).exists() {
            return std::fs::canonicalize(local_build)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| local_build.to_string());
        }
    }
    "track".to_string()
}

/// Check if a shell command runs track, either as `track` or by path
/// (e.g. `/path/to/track` or `./target/debug/track`)
pub(crate) fn is_track_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    // `track` must start the command or follow a path separator, so that
    // `echo track ...` or `mytrack ...` do not count.
    if let Some(track_idx) = trimmed.find("track ") {
        let before = &trimmed[..track_idx];
        if before.is_empty() || before.ends_with(['/', '\\']) {
            return true;
        }
    }
    trimmed == "track" || trimmed.ends_with("/track") || trimmed.ends_with("\\track")
}

/// Parse the arguments of a track shell command
pub(crate) fn parse_track_args(cmd: &str) -> Vec<String> {
    const TRACK_PREFIX: &str = "track ";

    let trimmed = cmd.trim();

    // Handle path-based track command
    let Some(idx) = trimmed.find(TRACK_PREFIX) else {
        // No "track " found - either ends with "track" (no args) or not a track command
        return vec![];
    };

    let track_part = &trimmed[idx + TRACK_PREFIX.len()..];

    // Fall back to whitespace splitting when the quoting is unbalanced
    shell_words::split(track_part).unwrap_or_else(|_| {
        track_part
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    })
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

    #[test]
    fn test_parse_track_input_missing_args() {
        let input = json!({});
        let result = parse_track_input(&input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Missing 'args' field");
    }

    #[test]
    fn test_parse_track_input_args_not_array() {
        let input = json!({
            "args": "issue get DEMO-1"
        });
        let result = parse_track_input(&input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "'args' must be an array");
    }

    #[test]
    fn test_parse_track_input_args_not_strings() {
        let input = json!({
            "args": ["issue", 123]
        });
        let result = parse_track_input(&input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "All args must be strings");
    }

    #[test]
    fn test_is_track_command() {
        assert!(is_track_command("track issue get DEMO-1"));
        assert!(is_track_command("  track issue get DEMO-1  "));
        assert!(is_track_command("/path/to/track issue get DEMO-1"));
        assert!(is_track_command("./target/debug/track issue list"));
        assert!(is_track_command("/path/to/track"));
        assert!(!is_track_command("ls -la"));
        assert!(!is_track_command("cat file.txt"));
        assert!(!is_track_command("git commit"));
        assert!(!is_track_command("echo track issue get DEMO-1"));
    }

    #[test]
    fn is_track_command_accepts_path_with_multibyte_characters() {
        // `find` returns a byte offset; using it as a char index looked at the
        // wrong character once the path contained multi-byte UTF-8.
        assert!(is_track_command("/home/josé/bin/track issue get DEMO-1"));
    }

    #[test]
    fn test_parse_track_args() {
        assert_eq!(
            parse_track_args("track issue get DEMO-1"),
            vec!["issue", "get", "DEMO-1"]
        );
        assert_eq!(
            parse_track_args("track issue comment DEMO-1 -m \"Hello world\""),
            vec!["issue", "comment", "DEMO-1", "-m", "Hello world"]
        );
        assert_eq!(
            parse_track_args("/path/to/track i s \"query\""),
            vec!["i", "s", "query"]
        );
    }
}
