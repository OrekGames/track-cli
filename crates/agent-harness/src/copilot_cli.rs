//! GitHub Copilot CLI runner
//!
//! Invokes GitHub Copilot CLI as a subprocess and captures its behavior for evaluation.
//! This allows testing how GitHub Copilot performs on scenarios using the track CLI.
//! Supports both legacy `gh copilot` mode and new standalone `copilot` agent mode.

use crate::runner::{CommandExecution, SessionResult};
use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;
use tracker_mock::Scenario;

/// Copilot CLI operation mode
#[derive(Debug, Clone, Copy)]
pub enum CopilotMode {
    /// Legacy gh copilot suggest mode (interactive Q&A)
    Suggest,
    /// New standalone copilot CLI agent mode
    #[allow(dead_code)]
    Agent,
}

/// Configuration for Copilot CLI invocation
pub struct CopilotCliConfig {
    pub scenario_path: PathBuf,
    pub scenario: Scenario,
    pub max_turns: usize,
    pub verbose: bool,
    pub copilot_mode: CopilotMode,
}

/// A single interaction with Copilot CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotInteraction {
    pub prompt: String,
    pub response: String,
    pub command_suggested: Option<String>,
    pub command_executed: bool,
}

/// Result of a Copilot CLI session
#[allow(dead_code)]
pub struct CopilotCliResult {
    pub turns_used: usize,
    pub commands_executed: Vec<CommandExecution>,
    pub duration_ms: u64,
    pub exit_code: i32,
    pub interactions: Vec<CopilotInteraction>,
    pub final_output: Option<String>,
}

/// Run Copilot CLI against a scenario
pub fn run_copilot_cli(config: &CopilotCliConfig) -> Result<CopilotCliResult> {
    let start = Instant::now();

    // Build system prompt
    let system_prompt = build_system_prompt(&config.scenario);

    // Find track binary path
    let track_bin = find_track_binary();
    let track_bin_dir = std::path::Path::new(&track_bin)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    if config.verbose {
        println!("\n{}", "Starting Copilot CLI session...".cyan());
        println!("{}: {}", "Scenario".dimmed(), config.scenario.scenario.name);
        println!("{}: {:?}", "Mode".dimmed(), config.copilot_mode);
        println!("{}: {}", "Max turns".dimmed(), config.max_turns);
        println!("{}: {}", "Track binary".dimmed(), track_bin);
        println!();
    }

    // Build command
    let mut cmd = build_copilot_command(config);

    // Set environment - route track commands through mock
    cmd.env("TRACK_MOCK_DIR", &config.scenario_path);

    // Add track binary to PATH if needed
    if !track_bin_dir.is_empty() {
        let current_path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{}:{}", track_bin_dir, current_path));
    }

    // Set up stdio
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Spawn process
    let mut child = cmd.spawn().context("Failed to spawn Copilot CLI")?;

    // Run interactive session
    let interactions = run_interactive_session(&mut child, &system_prompt, config)?;

    // Wait for completion
    let status = child.wait().context("Failed to wait for Copilot CLI")?;

    let duration_ms = start.elapsed().as_millis() as u64;

    // Extract commands from interactions
    let commands_executed = extract_track_commands(&interactions);
    let turns_used = interactions.len();
    let final_output = interactions.last().map(|i| i.response.clone());

    if config.verbose {
        println!(
            "\n{}: {} commands in {} turns",
            "Session complete".green(),
            commands_executed.len(),
            turns_used
        );
    }

    Ok(CopilotCliResult {
        turns_used,
        commands_executed,
        duration_ms,
        exit_code: status.code().unwrap_or(-1),
        interactions,
        final_output,
    })
}

/// Path to the skill file relative to the project root
const SKILL_FILE_PATH: &str = "agent-skills/SKILL.md";

/// Strip YAML frontmatter (--- ... ---) from skill file content
fn strip_frontmatter(content: &str) -> &str {
    if let Some(stripped) = content.strip_prefix("---\n") {
        if let Some(close) = stripped.find("\n---\n") {
            return stripped[close + 5..].trim_start();
        }
    }
    content
}

/// Load the skill file content, stripping frontmatter
fn load_skill_file() -> Option<String> {
    // Try to find the skill file relative to the current working directory
    let paths_to_try = [
        SKILL_FILE_PATH.to_string(),
        format!("./{}", SKILL_FILE_PATH),
        format!("../{}", SKILL_FILE_PATH),
        format!("../../{}", SKILL_FILE_PATH),
    ];

    for path in &paths_to_try {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Some(strip_frontmatter(&content).to_string());
        }
    }

    // Also try from CARGO_MANIFEST_DIR if available
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = std::path::Path::new(&manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // project root
            .map(|p| p.join(SKILL_FILE_PATH));

        if let Some(path) = path {
            if let Ok(content) = std::fs::read_to_string(&path) {
                return Some(strip_frontmatter(&content).to_string());
            }
        }
    }

    None
}

/// Build system prompt for Copilot CLI using the track skill file
fn build_system_prompt(scenario: &Scenario) -> String {
    let mut prompt = String::new();

    prompt.push_str("# Evaluation Mode\n\n");
    prompt.push_str(
        "You are being evaluated on your ability to use the `track` CLI efficiently and correctly.\n\n",
    );

    prompt.push_str("## Constraints\n\n");
    prompt.push_str("- You can ONLY use the `track` CLI\n");
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
        prompt.push_str("```\n\n");
    }

    if let Some(context) = &scenario.setup.context {
        prompt.push_str("## Scenario Context\n\n");
        prompt.push_str(context);
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Your Task\n\n");
    prompt.push_str(&scenario.setup.prompt);
    prompt.push_str("\n\n");

    prompt.push_str(
        "Complete this task using ONLY track CLI commands. When done, summarize what you did.\n",
    );

    prompt
}

/// Build the appropriate Copilot CLI command
fn build_copilot_command(config: &CopilotCliConfig) -> Command {
    match config.copilot_mode {
        CopilotMode::Suggest => {
            // Legacy: gh copilot suggest
            let mut cmd = Command::new("gh");
            cmd.args(["copilot", "suggest"]);
            cmd
        }
        CopilotMode::Agent => {
            // New: standalone copilot CLI
            // Add any agent-specific flags
            Command::new("copilot")
        }
    }
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

/// Run interactive session with Copilot CLI
fn run_interactive_session(
    child: &mut std::process::Child,
    system_prompt: &str,
    config: &CopilotCliConfig,
) -> Result<Vec<CopilotInteraction>> {
    let mut interactions = Vec::new();
    let mut stdin = child.stdin.take().context("Failed to get stdin")?;
    let stdout = child.stdout.take().context("Failed to get stdout")?;
    let mut reader = BufReader::new(stdout);

    // Send initial prompt
    if config.verbose {
        println!("{}: Sending task prompt", "Copilot".cyan());
    }

    writeln!(stdin, "{}", system_prompt)?;
    stdin.flush()?;

    let mut current_output = String::new();
    let mut turn_count = 0;
    let _waiting_for_response = true;

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                if config.verbose {
                    print!("{}", line);
                }

                current_output.push_str(&line);

                // Detect if Copilot is asking for input
                if detect_input_prompt(&line) {
                    turn_count += 1;
                    if turn_count > config.max_turns {
                        if config.verbose {
                            println!("\n{}: Max turns reached", "Stopping".yellow());
                        }
                        break;
                    }

                    // Check for command suggestion
                    if let Some(cmd) = extract_command_suggestion(&current_output) {
                        if is_track_command(&cmd) {
                            interactions.push(CopilotInteraction {
                                prompt: system_prompt.to_string(),
                                response: current_output.clone(),
                                command_suggested: Some(cmd.clone()),
                                command_executed: true,
                            });

                            // Auto-confirm execution
                            writeln!(stdin, "yes")?;
                            stdin.flush()?;

                            if config.verbose {
                                println!("{}: Auto-confirming track command", "Action".green());
                            }
                        } else {
                            // Reject non-track commands
                            writeln!(stdin, "no")?;
                            stdin.flush()?;

                            if config.verbose {
                                println!("{}: Rejecting non-track command", "Action".yellow());
                            }
                        }
                    } else {
                        // Send a generic response to continue
                        let response = generate_response(&current_output, config);
                        writeln!(stdin, "{}", response)?;
                        stdin.flush()?;
                    }

                    current_output.clear();
                }
            }
            Err(e) => {
                if config.verbose {
                    eprintln!("{}: {}", "Read error".red(), e);
                }
                break;
            }
        }
    }

    // Add final interaction if any
    if !current_output.is_empty() {
        interactions.push(CopilotInteraction {
            prompt: system_prompt.to_string(),
            response: current_output,
            command_suggested: None,
            command_executed: false,
        });
    }

    Ok(interactions)
}

/// Detect if a line indicates Copilot is waiting for user input
fn detect_input_prompt(line: &str) -> bool {
    let lower = line.to_lowercase();

    // Check for question marks with common question words
    if lower.contains("?") {
        return lower.contains("what")
            || lower.contains("how")
            || lower.contains("would you")
            || lower.contains("do you")
            || lower.contains("select")
            || lower.contains("choose");
    }

    // Check for prompts that end with : (like "Please describe your task:")
    if lower.ends_with(":") || lower.ends_with(":\n") {
        return lower.contains("please")
            || lower.contains("describe")
            || lower.contains("enter")
            || lower.contains("type")
            || lower.contains("select")
            || lower.contains("choose");
    }

    false
}

/// Extract command suggestion from Copilot output
fn extract_command_suggestion(output: &str) -> Option<String> {
    // Look for patterns like:
    // "$ track issue get DEMO-1"
    // "Suggestion: track issue get DEMO-1"
    // "  track issue get DEMO-1"

    for line in output.lines() {
        let trimmed = line.trim();

        // Pattern: $ command
        if let Some(stripped) = trimmed.strip_prefix("$ ") {
            return Some(stripped.trim().to_string());
        }

        // Pattern: Suggestion: command
        if let Some(stripped) = trimmed.strip_prefix("Suggestion:") {
            return Some(stripped.trim().to_string());
        }

        // Pattern: track command (direct)
        if trimmed.starts_with("track ") {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Generate an automated response to continue the conversation
fn generate_response(_output: &str, _config: &CopilotCliConfig) -> String {
    // Simple response to keep Copilot engaged
    "Yes, please proceed with the track CLI command.".to_string()
}

/// Check if a command is a track command
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

/// Extract track commands from interactions
fn extract_track_commands(interactions: &[CopilotInteraction]) -> Vec<CommandExecution> {
    let mut commands = Vec::new();

    for interaction in interactions {
        if let Some(cmd) = &interaction.command_suggested {
            if is_track_command(cmd) && interaction.command_executed {
                let args = parse_track_args(cmd);
                commands.push(CommandExecution {
                    args,
                    output: interaction.response.clone(),
                    is_error: false, // Would need to parse from output
                });
            }
        }
    }

    commands
}

impl From<CopilotCliResult> for SessionResult {
    fn from(result: CopilotCliResult) -> Self {
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
    fn test_detect_input_prompt() {
        assert!(detect_input_prompt("What would you like to do?"));
        assert!(detect_input_prompt("Please describe your task:"));
        assert!(detect_input_prompt("How can I help you?"));
        assert!(detect_input_prompt("Would you like to continue?"));
        assert!(detect_input_prompt("Select an option:"));
        assert!(!detect_input_prompt("Running command..."));
        assert!(!detect_input_prompt("Output: success"));
    }

    #[test]
    fn test_extract_command_suggestion() {
        assert_eq!(
            extract_command_suggestion("$ track issue get DEMO-1"),
            Some("track issue get DEMO-1".to_string())
        );
        assert_eq!(
            extract_command_suggestion("Suggestion: track issue list"),
            Some("track issue list".to_string())
        );
        assert_eq!(
            extract_command_suggestion("  track issue get DEMO-1  "),
            Some("track issue get DEMO-1".to_string())
        );
        assert_eq!(extract_command_suggestion("No command here"), None);
    }

    #[test]
    fn test_is_track_command() {
        assert!(is_track_command("track issue get DEMO-1"));
        assert!(is_track_command("  track issue get DEMO-1  "));
        assert!(is_track_command("/path/to/track issue get DEMO-1"));
        assert!(is_track_command("./target/debug/track issue list"));
        assert!(!is_track_command("ls -la"));
        assert!(!is_track_command("cat file.txt"));
        assert!(!is_track_command("git commit"));
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
