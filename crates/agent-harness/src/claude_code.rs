//! Claude Code CLI runner
//!
//! Invokes Claude Code as a subprocess and captures its behavior for evaluation.
//! This allows testing how Claude Code specifically performs on scenarios.

use crate::runner::{CommandExecution, SessionResult};
use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;
use tracker_mock::Scenario;

/// Configuration for Claude Code invocation
pub struct ClaudeCodeConfig {
    pub scenario_path: PathBuf,
    pub scenario: Scenario,
    pub max_turns: usize,
    pub verbose: bool,
}

/// Output event from Claude Code stream-json format
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ClaudeCodeEvent {
    #[serde(rename = "system")]
    System(SystemEvent),
    #[serde(rename = "assistant")]
    Assistant(AssistantEvent),
    #[serde(rename = "user")]
    User(UserEvent),
    #[serde(rename = "result")]
    Result(ResultEvent),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemEvent {
    pub subtype: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantEvent {
    #[serde(default)]
    pub message: Option<AssistantMessage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserEvent {
    #[serde(default)]
    pub message: Option<UserMessage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMessage {
    #[serde(default)]
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResultEvent {
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub duration_api_ms: Option<u64>,
    #[serde(default)]
    pub num_turns: Option<u32>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

/// Result of a Claude Code session
#[allow(dead_code)]
pub struct ClaudeCodeResult {
    pub turns_used: usize,
    pub events: Vec<ClaudeCodeEvent>,
    pub final_result: Option<String>,
    pub duration_ms: u64,
    pub exit_code: i32,
    pub commands_executed: Vec<CommandExecution>,
}

/// Run Claude Code against a scenario
pub fn run_claude_code(config: &ClaudeCodeConfig) -> Result<ClaudeCodeResult> {
    let start = Instant::now();

    // Build system prompt
    let system_prompt = build_system_prompt(&config.scenario);

    // Build the task prompt
    let task_prompt = build_task_prompt(&config.scenario);

    // Find track binary path
    let track_bin = find_track_binary();
    let track_bin_dir = std::path::Path::new(&track_bin)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    if config.verbose {
        println!("\n{}", "Starting Claude Code session...".cyan());
        println!("{}: {}", "Scenario".dimmed(), config.scenario.scenario.name);
        println!("{}: {}", "Max turns".dimmed(), config.max_turns);
        println!("{}: {}", "Track binary".dimmed(), track_bin);
        println!();
    }

    // Build command
    let mut cmd = Command::new("claude");
    cmd.args([
        "-p",
        "--output-format",
        "stream-json",
        "--verbose", // Required for stream-json with --print
        "--allowedTools",
        &format!("Bash({} *)", track_bin),
        "--append-system-prompt",
        &system_prompt,
        "--dangerously-skip-permissions",
        "--max-turns",
        &config.max_turns.to_string(),
        // Task prompt as final argument
        &task_prompt,
    ]);

    // Set environment - route track commands through mock
    cmd.env("TRACK_MOCK_DIR", &config.scenario_path);

    // Add track binary to PATH if needed
    if !track_bin_dir.is_empty() {
        let current_path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{}", track_bin_dir, current_path));
    }

    // Set up stdio
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Spawn process
    let mut child = cmd.spawn().context("Failed to spawn Claude Code CLI")?;

    // Parse streaming JSON output
    let stdout = child.stdout.take().context("Failed to capture stdout")?;
    let events = parse_stream_output(stdout, config.verbose)?;

    // Wait for completion
    let status = child.wait().context("Failed to wait for Claude Code")?;

    let duration_ms = start.elapsed().as_millis() as u64;

    // Extract commands from events
    let commands_executed = extract_commands(&events);
    let turns_used = count_turns(&events);
    let final_result = extract_final_result(&events);

    if config.verbose {
        println!(
            "\n{}: {} commands in {} turns",
            "Session complete".green(),
            commands_executed.len(),
            turns_used
        );
    }

    Ok(ClaudeCodeResult {
        turns_used,
        events,
        final_result,
        duration_ms,
        exit_code: status.code().unwrap_or(-1),
        commands_executed,
    })
}

/// Path to the agent guide relative to the project root
const AGENT_GUIDE_PATH: &str = "docs/agent_guide.md";

/// Load the agent guide content from the docs directory
fn load_agent_guide() -> Option<String> {
    // Try to find the agent guide relative to the current working directory
    let paths_to_try = [
        AGENT_GUIDE_PATH.to_string(),
        format!("./{}", AGENT_GUIDE_PATH),
        format!("../{}", AGENT_GUIDE_PATH),
        format!("../../{}", AGENT_GUIDE_PATH),
    ];

    for path in &paths_to_try {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Some(content);
        }
    }

    // Also try from CARGO_MANIFEST_DIR if available
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = std::path::Path::new(&manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // project root
            .map(|p| p.join(AGENT_GUIDE_PATH));

        if let Some(path) = path {
            if let Ok(content) = std::fs::read_to_string(&path) {
                return Some(content);
            }
        }
    }

    None
}

/// Build system prompt for Claude Code using the agent guide
fn build_system_prompt(scenario: &Scenario) -> String {
    let mut prompt = String::new();

    prompt.push_str("# Evaluation Mode\n\n");
    prompt.push_str(
        "You are being evaluated on your ability to use the `track` CLI efficiently and correctly.\n\n",
    );

    prompt.push_str("## Constraints\n\n");
    prompt.push_str("- You can ONLY use the `track` CLI via Bash\n");
    prompt.push_str("- Do NOT use any other tools, commands, or file operations\n");
    prompt.push_str("- Minimize the number of commands you execute\n");
    prompt.push_str("- Use `-o json` when you need to parse output programmatically\n\n");

    // Load and include the agent guide
    if let Some(agent_guide) = load_agent_guide() {
        prompt.push_str("---\n\n");
        prompt.push_str(&agent_guide);
        prompt.push_str("\n---\n\n");
    } else {
        // Fallback to minimal reference if agent guide not found
        prompt.push_str("## Track CLI Quick Reference\n\n");
        prompt.push_str("Note: Full agent guide not found. Using minimal reference.\n\n");
        prompt.push_str("```bash\n");
        prompt.push_str("track PROJ-123                     # Get issue\n");
        prompt.push_str("track i s \"project: PROJ #Unresolved\"  # Search issues\n");
        prompt.push_str("track i new -p PROJ -s \"Summary\"   # Create issue\n");
        prompt.push_str("track i u PROJ-123 --field \"Field=Value\"  # Update issue\n");
        prompt.push_str("track i del PROJ-123               # Delete issue\n");
        prompt.push_str("track i cmt PROJ-123 -m \"Comment\"  # Add comment\n");
        prompt.push_str("track i link PROJ-1 PROJ-2         # Link issues\n");
        prompt.push_str("track p ls                         # List projects\n");
        prompt.push_str("track p f PROJ                     # List custom fields\n");
        prompt.push_str("track a ls --project PROJ          # List articles\n");
        prompt.push_str("track t ls                         # List tags\n");
        prompt.push_str("```\n\n");
    }

    if let Some(context) = &scenario.setup.context {
        prompt.push_str("## Scenario Context\n\n");
        prompt.push_str(context);
        prompt.push_str("\n\n");
    }

    prompt.push_str(
        "When you have completed all tasks, respond with a brief summary of what you did.\n",
    );

    prompt
}

/// Build the task prompt from scenario
fn build_task_prompt(scenario: &Scenario) -> String {
    format!(
        "## Your Task\n\n{}\n\nPlease complete this task using the track CLI.",
        scenario.setup.prompt
    )
}

/// Find the track binary
fn find_track_binary() -> String {
    // Check for debug build first
    let debug_path = "./target/debug/track";
    if std::path::Path::new(debug_path).exists() {
        return std::fs::canonicalize(debug_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| debug_path.to_string());
    }

    // Check for release build
    let release_path = "./target/release/track";
    if std::path::Path::new(release_path).exists() {
        return std::fs::canonicalize(release_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| release_path.to_string());
    }

    // Fall back to PATH
    "track".to_string()
}

/// Parse streaming JSON output from Claude Code
fn parse_stream_output(stdout: impl std::io::Read, verbose: bool) -> Result<Vec<ClaudeCodeEvent>> {
    let reader = BufReader::new(stdout);
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = line.context("Failed to read line from stdout")?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<ClaudeCodeEvent>(&line) {
            Ok(event) => {
                if verbose {
                    print_event(&event);
                }
                events.push(event);
            }
            Err(_) => {
                // Try parsing as a generic JSON to see what we got
                if verbose {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                        if let Some(t) = v.get("type") {
                            eprintln!("{}: Unknown event type: {}", "Debug".dimmed(), t);
                        }
                    }
                }
            }
        }
    }

    Ok(events)
}

/// Print an event in verbose mode
fn print_event(event: &ClaudeCodeEvent) {
    match event {
        ClaudeCodeEvent::System(sys) => {
            if sys.subtype == "init" {
                if let Some(msg) = &sys.message {
                    println!("{}: {}", "System".dimmed(), msg);
                }
            }
        }
        ClaudeCodeEvent::Assistant(asst) => {
            if let Some(msg) = &asst.message {
                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } => {
                            println!("{}: {}", "Agent".green(), text);
                        }
                        ContentBlock::ToolUse { name, input, .. } => {
                            if name == "Bash" {
                                if let Some(cmd) = input.get("command").and_then(|c| c.as_str()) {
                                    println!("{}: {}", "Executing".yellow(), cmd);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        ClaudeCodeEvent::User(user) => {
            if let Some(msg) = &user.message {
                for block in &msg.content {
                    if let ContentBlock::ToolResult {
                        content, is_error, ..
                    } = block
                    {
                        let label = if *is_error {
                            "Error".red()
                        } else {
                            "Output".dimmed()
                        };
                        // Truncate long output
                        let display = if content.len() > 300 {
                            format!("{}...", &content[..300])
                        } else {
                            content.clone()
                        };
                        println!("{}: {}", label, display.trim());
                    }
                }
            }
        }
        ClaudeCodeEvent::Result(res) => {
            if let Some(result) = &res.result {
                println!("{}: {}", "Final result".cyan(), result);
            }
        }
    }
}

/// Extract track commands from events
fn extract_commands(events: &[ClaudeCodeEvent]) -> Vec<CommandExecution> {
    let mut commands = Vec::new();
    let mut pending_commands: Vec<(String, String)> = Vec::new(); // (tool_use_id, command)

    for event in events {
        match event {
            ClaudeCodeEvent::Assistant(asst) => {
                if let Some(msg) = &asst.message {
                    for block in &msg.content {
                        if let ContentBlock::ToolUse { id, name, input } = block {
                            if name == "Bash" {
                                if let Some(cmd) = input.get("command").and_then(|c| c.as_str()) {
                                    if is_track_command(cmd) {
                                        pending_commands.push((id.clone(), cmd.to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ClaudeCodeEvent::User(user) => {
                if let Some(msg) = &user.message {
                    for block in &msg.content {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } = block
                        {
                            // Find matching pending command
                            if let Some(idx) = pending_commands
                                .iter()
                                .position(|(id, _)| id == tool_use_id)
                            {
                                let (_, cmd) = pending_commands.remove(idx);
                                let args = parse_track_args(&cmd);
                                commands.push(CommandExecution {
                                    args,
                                    output: content.clone(),
                                    is_error: *is_error,
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    commands
}

/// Check if a bash command is a track command
fn is_track_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    // Check for direct track command
    if trimmed.starts_with("track ") || trimmed == "track" {
        return true;
    }
    // Check for path-based track command (e.g., /path/to/track or ./target/debug/track)
    if trimmed.contains("track ") {
        // Extract the command name from a potential path
        if let Some(track_idx) = trimmed.find("track ") {
            // Verify "track" is preceded by a path separator or start of string
            if track_idx == 0 {
                return true;
            }
            let prev_char = trimmed.chars().nth(track_idx - 1);
            if prev_char == Some('/') || prev_char == Some('\\') {
                return true;
            }
        }
    }
    // Check for track at end of path without args
    if trimmed.ends_with("/track") || trimmed.ends_with("\\track") {
        return true;
    }
    false
}

/// Parse track command into args
fn parse_track_args(cmd: &str) -> Vec<String> {
    let trimmed = cmd.trim();

    // Handle path-based track command
    let Some(idx) = trimmed.find("track ") else {
        // No "track " found - either ends with "track" (no args) or not a track command
        return vec![];
    };

    let track_part = &trimmed[idx + 6..];

    // Simple arg splitting (doesn't handle all quoting edge cases)
    shell_words::split(track_part).unwrap_or_else(|_| {
        track_part
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    })
}

/// Count the number of turns (assistant responses)
fn count_turns(events: &[ClaudeCodeEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, ClaudeCodeEvent::Assistant(_)))
        .count()
}

/// Extract final result text
fn extract_final_result(events: &[ClaudeCodeEvent]) -> Option<String> {
    events.iter().rev().find_map(|e| {
        if let ClaudeCodeEvent::Result(res) = e {
            res.result.clone()
        } else {
            None
        }
    })
}

/// Convert ClaudeCodeResult to SessionResult for evaluation
impl From<ClaudeCodeResult> for SessionResult {
    fn from(result: ClaudeCodeResult) -> Self {
        SessionResult {
            turns_used: result.turns_used,
            total_input_tokens: 0,  // Not available from CLI
            total_output_tokens: 0, // Not available from CLI
            commands_executed: result.commands_executed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_track_command() {
        assert!(is_track_command("track issue get DEMO-1"));
        assert!(is_track_command("  track issue get DEMO-1  "));
        assert!(is_track_command("/path/to/track issue get DEMO-1"));
        assert!(!is_track_command("ls -la"));
        assert!(!is_track_command("cat file.txt"));
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
    }

    #[test]
    fn test_parse_assistant_event() {
        let json = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"I'll help you."}]}}"#;
        let event: ClaudeCodeEvent = serde_json::from_str(json).unwrap();
        if let ClaudeCodeEvent::Assistant(asst) = event {
            assert!(asst.message.is_some());
        } else {
            panic!("Expected Assistant event");
        }
    }

    #[test]
    fn test_parse_tool_use() {
        let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"123","name":"Bash","input":{"command":"track issue get DEMO-1"}}]}}"#;
        let event: ClaudeCodeEvent = serde_json::from_str(json).unwrap();
        if let ClaudeCodeEvent::Assistant(asst) = event {
            let msg = asst.message.unwrap();
            if let ContentBlock::ToolUse { name, input, .. } = &msg.content[0] {
                assert_eq!(name, "Bash");
                assert_eq!(
                    input.get("command").and_then(|c| c.as_str()),
                    Some("track issue get DEMO-1")
                );
            } else {
                panic!("Expected ToolUse block");
            }
        } else {
            panic!("Expected Assistant event");
        }
    }
}
