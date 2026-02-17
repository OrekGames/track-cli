//! Gemini CLI runner (stub)
//!
//! Placeholder for running Gemini CLI as an agent.

use crate::runner::{CommandExecution, SessionResult};
use anyhow::Result;
use std::path::PathBuf;
use tracker_mock::Scenario;

/// Configuration for Gemini runner invocation
pub struct GeminiRunnerConfig {
    pub scenario_path: PathBuf,
    pub scenario: Scenario,
    pub verbose: bool,
}

/// Result of a Gemini session
pub struct GeminiRunnerResult {
    pub turns_used: usize,
    pub commands_executed: Vec<CommandExecution>,
}

/// Run Gemini against a scenario
pub fn run_gemini(_config: &GeminiRunnerConfig) -> Result<GeminiRunnerResult> {
    Err(anyhow::anyhow!("Gemini runner is not yet implemented"))
}

impl From<GeminiRunnerResult> for SessionResult {
    fn from(result: GeminiRunnerResult) -> Self {
        SessionResult {
            turns_used: result.turns_used,
            total_input_tokens: 0,
            total_output_tokens: 0,
            commands_executed: result.commands_executed,
        }
    }
}
