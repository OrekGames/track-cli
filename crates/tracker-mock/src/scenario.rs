//! Scenario definition and loading
//!
//! A scenario defines the test setup, expected outcomes, and scoring criteria.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A complete test scenario for AI agent evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    /// Scenario metadata
    pub scenario: ScenarioMeta,

    /// Setup information for the agent
    pub setup: SetupConfig,

    /// Expected outcomes to verify
    #[serde(default)]
    pub expected_outcomes: HashMap<String, ExpectedOutcome>,

    /// Scoring configuration
    #[serde(default)]
    pub scoring: ScoringConfig,
}

/// Scenario metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioMeta {
    /// Unique scenario name
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// Target backend (youtrack, jira, or any)
    #[serde(default = "default_backend")]
    pub backend: String,

    /// Difficulty level (easy, medium, hard)
    #[serde(default = "default_difficulty")]
    pub difficulty: String,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_backend() -> String {
    "any".to_string()
}

fn default_difficulty() -> String {
    "medium".to_string()
}

/// Setup configuration for the scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupConfig {
    /// The task prompt to give to the AI agent
    pub prompt: String,

    /// Default project for the scenario
    #[serde(default)]
    pub default_project: Option<String>,

    /// Additional context to provide
    #[serde(default)]
    pub context: Option<String>,

    /// Whether cache should be pre-populated
    #[serde(default)]
    pub cache_available: bool,
}

/// An expected outcome to verify after the scenario runs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExpectedOutcome {
    /// Simple boolean check (e.g., "issue_resolved": true)
    Boolean(bool),

    /// String value to match (e.g., "issue_fetched": "DEMO-1")
    String(String),

    /// Complex outcome with multiple criteria
    Complex(ComplexOutcome),
}

/// Complex outcome with multiple match criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexOutcome {
    /// Issue ID this outcome relates to
    #[serde(default)]
    pub issue: Option<String>,

    /// Field name that should be changed
    #[serde(default)]
    pub field: Option<String>,

    /// Expected value
    #[serde(default)]
    pub value: Option<String>,

    /// Comment should contain this text
    #[serde(default)]
    pub contains: Option<String>,

    /// Method should have been called
    #[serde(default)]
    pub method_called: Option<String>,

    /// Minimum number of times method was called
    #[serde(default)]
    pub min_calls: Option<usize>,

    /// Maximum number of times method was called
    #[serde(default)]
    pub max_calls: Option<usize>,
}

/// Scoring configuration for evaluation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScoringConfig {
    /// Minimum commands needed (theoretical optimum)
    #[serde(default)]
    pub min_commands: Option<usize>,

    /// Maximum acceptable commands before penalty
    #[serde(default)]
    pub max_commands: Option<usize>,

    /// Optimal number of commands (good agent behavior)
    #[serde(default)]
    pub optimal_commands: Option<usize>,

    /// Point penalties
    #[serde(default)]
    pub penalties: PenaltyConfig,

    /// Point bonuses
    #[serde(default)]
    pub bonuses: BonusConfig,

    /// Base score (default: 100)
    #[serde(default = "default_base_score")]
    pub base_score: i32,
}

fn default_base_score() -> i32 {
    100
}

/// Penalty point deductions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PenaltyConfig {
    /// Points deducted per command over max_commands
    #[serde(default = "default_extra_penalty")]
    pub extra_command: i32,

    /// Points deducted for redundant fetches (same resource twice)
    #[serde(default = "default_redundant_penalty")]
    pub redundant_fetch: i32,

    /// Points deducted for unnecessary list operations
    #[serde(default)]
    pub unnecessary_list: i32,

    /// Points deducted for failed commands
    #[serde(default = "default_error_penalty")]
    pub command_error: i32,
}

fn default_extra_penalty() -> i32 {
    -5
}

fn default_redundant_penalty() -> i32 {
    -10
}

fn default_error_penalty() -> i32 {
    -15
}

/// Bonus point additions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BonusConfig {
    /// Points added for using cache effectively
    #[serde(default = "default_cache_bonus")]
    pub cache_use: i32,

    /// Points added for completing under optimal commands
    #[serde(default)]
    pub under_optimal: i32,

    /// Points added for using JSON output mode
    #[serde(default)]
    pub json_output: i32,
}

fn default_cache_bonus() -> i32 {
    10
}

impl Scenario {
    /// Load a scenario from a TOML file
    pub fn load(path: &Path) -> Result<Self, ScenarioError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| ScenarioError::Io(path.to_path_buf(), e))?;

        toml::from_str(&content).map_err(|e| ScenarioError::Parse(path.to_path_buf(), e))
    }

    /// Load a scenario from a directory (looks for scenario.toml)
    pub fn load_from_dir(dir: &Path) -> Result<Self, ScenarioError> {
        let scenario_file = dir.join("scenario.toml");
        Self::load(&scenario_file)
    }

    /// Get the agent prompt for this scenario
    pub fn agent_prompt(&self) -> &str {
        &self.setup.prompt
    }

    /// Check if this scenario is compatible with a given backend
    pub fn is_compatible_with(&self, backend: &str) -> bool {
        self.scenario.backend == "any" || self.scenario.backend.eq_ignore_ascii_case(backend)
    }
}

/// Errors that can occur when loading a scenario
#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("Failed to read scenario file {0}: {1}")]
    Io(std::path::PathBuf, std::io::Error),

    #[error("Failed to parse scenario {0}: {1}")]
    Parse(std::path::PathBuf, toml::de::Error),

    #[error("Scenario directory not found: {0}")]
    DirNotFound(std::path::PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_parse() {
        let toml = r#"
[scenario]
name = "basic-workflow"
description = "Test basic issue workflow"
backend = "youtrack"
difficulty = "easy"
tags = ["issues", "comments"]

[setup]
prompt = "Find issue DEMO-1 and add a comment"
default_project = "DEMO"
cache_available = true

[expected_outcomes]
issue_fetched = "DEMO-1"
comment_added = { issue = "DEMO-1", contains = "test" }

[scoring]
min_commands = 2
max_commands = 5
optimal_commands = 3
base_score = 100

[scoring.penalties]
extra_command = -5
redundant_fetch = -10

[scoring.bonuses]
cache_use = 10
"#;

        let scenario: Scenario = toml::from_str(toml).unwrap();
        assert_eq!(scenario.scenario.name, "basic-workflow");
        assert_eq!(scenario.scenario.backend, "youtrack");
        assert!(scenario.setup.cache_available);
        assert_eq!(scenario.scoring.min_commands, Some(2));
    }

    #[test]
    fn test_backend_compatibility() {
        let scenario = Scenario {
            scenario: ScenarioMeta {
                name: "test".to_string(),
                description: "test".to_string(),
                backend: "any".to_string(),
                difficulty: "easy".to_string(),
                tags: vec![],
            },
            setup: SetupConfig {
                prompt: "test".to_string(),
                default_project: None,
                context: None,
                cache_available: false,
            },
            expected_outcomes: HashMap::new(),
            scoring: ScoringConfig::default(),
        };

        assert!(scenario.is_compatible_with("youtrack"));
        assert!(scenario.is_compatible_with("jira"));
    }
}
