//! Mock client for tracker-cli evaluation and testing
//!
//! This crate provides a mock implementation of the `IssueTracker` trait that reads
//! responses from fixture files instead of making real HTTP requests. It's designed
//! for:
//!
//! 1. **AI Agent Evaluation**: Test how efficiently and correctly AI agents use the CLI
//! 2. **Integration Testing**: Run tests without a real tracker instance
//! 3. **Reproducible Scenarios**: Create deterministic test cases
//!
//! # Usage
//!
//! Set the `TRACK_MOCK_DIR` environment variable to point to a scenario directory:
//!
//! ```bash
//! TRACK_MOCK_DIR=./fixtures/scenarios/basic-workflow track issue get DEMO-1
//! ```
//!
//! # Scenario Structure
//!
//! ```text
//! scenarios/basic-workflow/
//! ├── scenario.toml      # Metadata and evaluation criteria
//! ├── manifest.toml      # Request → response mapping
//! ├── call_log.jsonl     # Runtime log (written by MockClient)
//! └── responses/         # JSON response files
//!     ├── get_issue_DEMO-1.json
//!     └── list_projects.json
//! ```

mod client;
mod evaluator;
mod manifest;
mod scenario;

pub use client::{log_cli_command, CallLogEntry, MockClient};
pub use evaluator::{EvaluationResult, Evaluator};
pub use manifest::{Manifest, ResponseMapping};
pub use scenario::{ExpectedOutcome, Scenario, ScoringConfig};

/// Environment variable to enable mock mode
pub const MOCK_DIR_ENV: &str = "TRACK_MOCK_DIR";

/// Check if mock mode is enabled via environment variable
pub fn is_mock_enabled() -> bool {
    std::env::var(MOCK_DIR_ENV).is_ok()
}

/// Get the mock directory from environment, if set
pub fn get_mock_dir() -> Option<std::path::PathBuf> {
    std::env::var(MOCK_DIR_ENV)
        .ok()
        .map(std::path::PathBuf::from)
}
