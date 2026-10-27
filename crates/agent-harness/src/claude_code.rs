//! Claude Code CLI runner
//!
//! Invokes Claude Code as a subprocess and captures its behavior for evaluation.
//! This allows testing how Claude Code specifically performs on scenarios.
//! This is intended to be used in ephemeral or test environments and runs claude
//! without requiring it request permissions. Use caution when running it against
//! your local machine.

use crate::permissions::format_allowed_bash_pattern;
use crate::runner::{CommandExecution, SessionResult, load_skill_file};
use crate::subprocess::{StderrCapture, ensure_success, route_track_to_mock};
use crate::tools::{find_track_binary, is_track_command, parse_track_args};
use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
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
    /// Blocks the harness does not inspect, such as `thinking`. Without this,
    /// one such block would fail the whole event, dropping its tool calls.
    #[serde(other)]
    Other,
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
    /// `success`, or why the session ended early (e.g. `error_max_turns`)
    #[serde(default)]
    pub subtype: Option<String>,
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

/// Run Claude Code against a scenario
pub fn run_claude_code(config: &ClaudeCodeConfig) -> Result<SessionResult> {
    // Build system prompt
    let system_prompt = build_system_prompt(&config.scenario);

    // Build the task prompt
    let task_prompt = build_task_prompt(&config.scenario);

    // Find track binary path
    let track_bin = find_track_binary();

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
        &format_allowed_bash_pattern(&track_bin),
        "--append-system-prompt",
        &system_prompt,
        "--dangerously-skip-permissions",
        "--max-turns",
        &config.max_turns.to_string(),
        // Task prompt as final argument
        &task_prompt,
    ]);

    route_track_to_mock(&mut cmd, &config.scenario_path, &track_bin);

    // Unset CLAUDECODE to allow running inside a Claude Code session
    cmd.env_remove("CLAUDECODE");

    run_stream_json_session(cmd, "Claude Code CLI", config.verbose)
}

/// Run an agent CLI that prints Claude Code stream-json events on stdout, and
/// collect the track commands it executed.
pub(crate) fn run_stream_json_session(
    mut cmd: Command,
    cli_name: &str,
    verbose: bool,
) -> Result<SessionResult> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn {cli_name}"))?;
    let stderr = StderrCapture::start(&mut child);

    // Parse streaming JSON output
    let stdout = child.stdout.take().context("Failed to capture stdout")?;
    let events = parse_stream_output(stdout, verbose)?;

    let status = child
        .wait()
        .with_context(|| format!("Failed to wait for {cli_name}"))?;
    // Running out of turns is an outcome to score, not a CLI failure, even if
    // the CLI exits non-zero for it.
    if !stopped_at_turn_limit(&events) {
        ensure_success(cli_name, status, stderr)?;
    }

    // Extract commands from events
    let commands_executed = extract_commands(&events);
    let turns_used = count_turns(&events);

    if verbose {
        println!(
            "\n{}: {} commands in {} turns",
            "Session complete".green(),
            commands_executed.len(),
            turns_used
        );
    }

    Ok(SessionResult {
        turns_used,
        total_input_tokens: None,  // Not available from CLI
        total_output_tokens: None, // Not available from CLI
        commands_executed,
    })
}

/// Build system prompt for Claude Code using the track skill file
pub(crate) fn build_system_prompt(scenario: &Scenario) -> String {
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

    // Load and include the skill file (same context a real user would have installed)
    if let Some(skill_content) = load_skill_file() {
        prompt.push_str("---\n\n");
        prompt.push_str(&skill_content);
        prompt.push_str("\n---\n\n");
    } else {
        // Fallback to minimal reference if skill file not found
        prompt.push_str("## Track CLI Quick Reference\n\n");
        prompt.push_str("Note: Skill file not found. Using minimal reference.\n\n");
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
pub(crate) fn build_task_prompt(scenario: &Scenario) -> String {
    format!(
        "## Your Task\n\n{}\n\nPlease complete this task using the track CLI.",
        scenario.setup.prompt
    )
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
                if verbose
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(&line)
                    && let Some(t) = v.get("type")
                {
                    eprintln!("{}: Unknown event type: {}", "Debug".dimmed(), t);
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
            if sys.subtype == "init"
                && let Some(msg) = &sys.message
            {
                println!("{}: {}", "System".dimmed(), msg);
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
                            if name == "Bash"
                                && let Some(cmd) = input.get("command").and_then(|c| c.as_str())
                            {
                                println!("{}: {}", "Executing".yellow(), cmd);
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
                        // Truncate long output.
                        let display = crate::text::truncate_for_display(content, 300);
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
                        if let ContentBlock::ToolUse { id, name, input } = block
                            && name == "Bash"
                            && let Some(cmd) = input.get("command").and_then(|c| c.as_str())
                            && is_track_command(cmd)
                        {
                            pending_commands.push((id.clone(), cmd.to_string()));
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
                                    is_error: Some(*is_error),
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

/// Count the number of turns (assistant responses)
fn count_turns(events: &[ClaudeCodeEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, ClaudeCodeEvent::Assistant(_)))
        .count()
}

/// Whether the session ended because it used up `--max-turns`
fn stopped_at_turn_limit(events: &[ClaudeCodeEvent]) -> bool {
    events.iter().any(|event| {
        matches!(event, ClaudeCodeEvent::Result(result)
            if result.subtype.as_deref() == Some("error_max_turns"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn unknown_content_block_keeps_tool_calls_in_same_event() {
        let stream = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"..."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"track issue get DEMO-1"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"DEMO-1: Fix login"}]}}"#,
            "\n",
        );
        let events = parse_stream_output(stream.as_bytes(), false).unwrap();
        let commands = extract_commands(&events);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].args, vec!["issue", "get", "DEMO-1"]);
    }

    #[cfg(unix)]
    fn run_fake_cli(script: &str) -> Result<SessionResult> {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script]);
        run_stream_json_session(cmd, "Fake CLI", false)
    }

    #[cfg(unix)]
    #[test]
    fn turn_limit_exit_is_scored_not_an_error() {
        let session = run_fake_cli(concat!(
            r#"echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"track issue get DEMO-1"}}]}}';"#,
            r#"echo '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}';"#,
            r#"echo '{"type":"result","subtype":"error_max_turns"}';"#,
            "exit 1",
        ))
        .expect("a --max-turns stop should be scored");
        assert_eq!(session.commands_executed.len(), 1);
        assert_eq!(session.turns_used, 1);
    }

    #[cfg(unix)]
    #[test]
    fn crash_is_an_error_that_quotes_stderr() {
        let err = run_fake_cli("echo 'Invalid API key' >&2; exit 1")
            .err()
            .expect("a crashed CLI should fail the run");
        assert!(err.to_string().ends_with("Invalid API key"), "{err}");
    }
}
