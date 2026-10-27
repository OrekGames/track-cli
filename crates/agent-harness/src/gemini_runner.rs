//! Gemini CLI runner
//!
//! Invokes Gemini CLI as a subprocess and captures its behavior for evaluation.
//! This allows testing how Gemini performs on scenarios using the track CLI.
//! Gemini gets the same prompts as Claude Code, and its stream-json output is
//! parsed with the Claude Code event schema.

use crate::claude_code::{build_system_prompt, build_task_prompt, run_stream_json_session};
use crate::permissions::format_allowed_bash_pattern;
use crate::runner::SessionResult;
use crate::subprocess::route_track_to_mock;
use crate::tools::find_track_binary;
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use std::process::Command;
use tracker_mock::Scenario;

/// Configuration for Gemini CLI invocation
pub struct GeminiRunnerConfig {
    pub scenario_path: PathBuf,
    pub scenario: Scenario,
    pub max_turns: usize,
    pub verbose: bool,
}

/// Run Gemini against a scenario
pub fn run_gemini(config: &GeminiRunnerConfig) -> Result<SessionResult> {
    // Find track binary path
    let track_bin = find_track_binary();

    if config.verbose {
        println!("\n{}", "Starting Gemini CLI session...".cyan());
        println!("{}: {}", "Scenario".dimmed(), config.scenario.scenario.name);
        println!("{}: {}", "Max turns".dimmed(), config.max_turns);
        println!("{}: {}", "Track binary".dimmed(), track_bin);
        println!();
    }

    // Combine system and task prompts for Gemini
    let full_prompt = format!(
        "{}\n\n{}",
        build_system_prompt(&config.scenario),
        build_task_prompt(&config.scenario)
    );

    // Build command
    let mut cmd = Command::new("gemini");
    cmd.args([
        "-p", // Non-interactive prompt mode
        &full_prompt,
        "-o",
        "stream-json", // Structured JSON output
        "--yolo",      // Auto-approve all tools
        "--allowed-tools",
        &format_allowed_bash_pattern(&track_bin), // Only allow track CLI
    ]);

    route_track_to_mock(&mut cmd, &config.scenario_path, &track_bin);

    run_stream_json_session(cmd, "Gemini CLI", config.verbose)
}
