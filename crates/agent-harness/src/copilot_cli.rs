//! GitHub Copilot CLI runner (stub)
//!
//! Placeholder for running GitHub Copilot CLI as an agent.

use crate::runner::{CommandExecution, SessionResult};
use anyhow::Result;
use std::path::PathBuf;
use tracker_mock::Scenario;

/// Configuration for Copilot CLI invocation
pub struct CopilotCliConfig {
    pub scenario_path: PathBuf,
    pub scenario: Scenario,
    pub max_turns: usize,
    pub verbose: bool,
}

/// Result of a Copilot CLI session
pub struct CopilotCliResult {
    pub turns_used: usize,
    pub commands_executed: Vec<CommandExecution>,
}

/// Run Copilot CLI against a scenario
pub fn run_copilot_cli(_config: &CopilotCliConfig) -> Result<CopilotCliResult> {
    Err(anyhow::anyhow!("Copilot CLI runner is not yet implemented"))
}

impl From<CopilotCliResult> for SessionResult {
    fn from(result: CopilotCliResult) -> Self {
        SessionResult {
            turns_used: result.turns_used,
            total_input_tokens: 0,
            total_output_tokens: 0,
            commands_executed: result.commands_executed,
        }
    }
}
