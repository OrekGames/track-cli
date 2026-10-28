//! GitHub Copilot CLI runner
//!
//! Invokes GitHub Copilot CLI as a subprocess and captures its behavior for evaluation.
//! This allows testing how GitHub Copilot performs on scenarios using the track CLI.
//! Drives the interactive Q&A of the legacy `gh copilot suggest` command.

use crate::runner::{CommandExecution, SessionResult, load_skill_file};
use crate::subprocess::{StderrCapture, ensure_success, route_track_to_mock};
use crate::tools::{find_track_binary, is_track_command, parse_track_args};
use anyhow::{Context, Result};
use colored::Colorize;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use tracker_mock::Scenario;

/// Reply to a Copilot question that is not a command confirmation, to keep it going
const CONTINUE_REPLY: &str = "Yes, please proceed with the track CLI command.";

/// Copilot CLI operation mode
#[derive(Debug, Clone, Copy)]
pub enum CopilotMode {
    /// Legacy gh copilot suggest mode (interactive Q&A)
    Suggest,
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
#[derive(Debug)]
struct CopilotInteraction {
    response: String,
    /// The track command Copilot suggested and the harness confirmed
    command_suggested: Option<String>,
}

/// The interactions of a Copilot session, and whether the harness cut it short
struct InteractiveSession {
    interactions: Vec<CopilotInteraction>,
    reached_turn_limit: bool,
}

/// Run Copilot CLI against a scenario
pub fn run_copilot_cli(config: &CopilotCliConfig) -> Result<SessionResult> {
    // Build system prompt
    let system_prompt = build_system_prompt(&config.scenario);

    // Find track binary path
    let track_bin = find_track_binary();

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
    route_track_to_mock(&mut cmd, &config.scenario_path, &track_bin);

    let interactions = run_session(cmd, &system_prompt, config.max_turns, config.verbose)?;

    // Extract commands from interactions
    let commands_executed = extract_track_commands(&interactions);
    let turns_used = interactions.len();

    if config.verbose {
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
    }
}

/// Spawn Copilot, run its interactive session, and reap the process
fn run_session(
    mut cmd: Command,
    system_prompt: &str,
    max_turns: usize,
    verbose: bool,
) -> Result<Vec<CopilotInteraction>> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().context("Failed to spawn Copilot CLI")?;
    let stderr = StderrCapture::start(&mut child);

    // Our ends of Copilot's stdin and stdout are closed when this returns, so
    // Copilot cannot stay blocked on them while we wait for it below.
    let session = run_interactive_session(&mut child, system_prompt, max_turns, verbose);

    let reached_turn_limit = matches!(&session, Ok(s) if s.reached_turn_limit);
    if reached_turn_limit {
        // The harness ended the session, so stop Copilot instead of waiting
        // for it to give up on its closed pipes.
        let _ = child.kill();
    }
    let status = child.wait().context("Failed to wait for Copilot CLI")?;
    // After a kill the exit status is meaningless. Otherwise check it before
    // the session result: if Copilot died, its stderr explains a session error
    // such as a broken pipe.
    if !reached_turn_limit {
        ensure_success("Copilot CLI", status, stderr)?;
    }

    Ok(session?.interactions)
}

/// Run interactive session with Copilot CLI
fn run_interactive_session(
    child: &mut Child,
    system_prompt: &str,
    max_turns: usize,
    verbose: bool,
) -> Result<InteractiveSession> {
    let mut interactions = Vec::new();
    let mut reached_turn_limit = false;
    let mut stdin = child.stdin.take().context("Failed to get stdin")?;
    let stdout = child.stdout.take().context("Failed to get stdout")?;
    let mut reader = BufReader::new(stdout);

    // Send initial prompt
    if verbose {
        println!("{}: Sending task prompt", "Copilot".cyan());
    }

    writeln!(stdin, "{}", system_prompt)?;
    stdin.flush()?;

    let mut current_output = String::new();
    let mut turn_count = 0;

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                if verbose {
                    print!("{}", line);
                }

                current_output.push_str(&line);

                // Detect if Copilot is asking for input
                if detect_input_prompt(&line) {
                    turn_count += 1;
                    if turn_count > max_turns {
                        if verbose {
                            println!("\n{}: Max turns reached", "Stopping".yellow());
                        }
                        reached_turn_limit = true;
                        break;
                    }

                    // Check for command suggestion
                    if let Some(cmd) = extract_command_suggestion(&current_output) {
                        if is_track_command(&cmd) {
                            interactions.push(CopilotInteraction {
                                response: current_output.clone(),
                                command_suggested: Some(cmd),
                            });

                            // Auto-confirm execution
                            writeln!(stdin, "yes")?;
                            stdin.flush()?;

                            if verbose {
                                println!("{}: Auto-confirming track command", "Action".green());
                            }
                        } else {
                            // Reject non-track commands
                            writeln!(stdin, "no")?;
                            stdin.flush()?;

                            if verbose {
                                println!("{}: Rejecting non-track command", "Action".yellow());
                            }
                        }
                    } else {
                        writeln!(stdin, "{}", CONTINUE_REPLY)?;
                        stdin.flush()?;
                    }

                    current_output.clear();
                }
            }
            Err(e) => {
                if verbose {
                    eprintln!("{}: {}", "Read error".red(), e);
                }
                break;
            }
        }
    }

    // Add final interaction if any
    if !current_output.is_empty() {
        interactions.push(CopilotInteraction {
            response: current_output,
            command_suggested: None,
        });
    }

    Ok(InteractiveSession {
        interactions,
        reached_turn_limit,
    })
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

/// Extract the confirmed track commands from interactions
fn extract_track_commands(interactions: &[CopilotInteraction]) -> Vec<CommandExecution> {
    interactions
        .iter()
        .filter_map(|interaction| {
            let cmd = interaction.command_suggested.as_ref()?;
            Some(CommandExecution {
                args: parse_track_args(cmd),
                output: interaction.response.clone(),
                is_error: None, // Would need to parse from output
            })
        })
        .collect()
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

    #[cfg(unix)]
    fn fake_copilot(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[cfg(unix)]
    #[test]
    fn turn_limit_keeps_interactions_when_copilot_exits_with_error() {
        // Suggests a track command after every answer, and fails once its
        // stdin closes, as an interactive CLI may when the harness stops it.
        let copilot = fake_copilot(
            r#"while read -r answer; do
                 echo '$ track issue get DEMO-1'
                 echo 'What would you like to do?'
               done
               exit 1"#,
        );
        let interactions = run_session(copilot, "task", 1, false)
            .expect("stopping at the turn limit should keep the session");
        let commands = extract_track_commands(&interactions);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].args, vec!["issue", "get", "DEMO-1"]);
    }

    #[cfg(unix)]
    #[test]
    fn copilot_failure_is_an_error_that_quotes_stderr() {
        let copilot = fake_copilot(r#"echo 'unknown command "copilot" for "gh"' >&2; exit 1"#);
        let err = run_session(copilot, "task", 5, false).unwrap_err();
        assert!(
            err.to_string()
                .ends_with(r#"unknown command "copilot" for "gh""#),
            "{err}"
        );
    }
}
