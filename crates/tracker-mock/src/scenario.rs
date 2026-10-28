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
///
/// Omitted keys (and an omitted `[scoring]` table) take the values from
/// [`Default`], so a partial table and a missing one score alike.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoringConfig {
    /// Minimum commands needed (theoretical optimum)
    pub min_commands: Option<usize>,

    /// Maximum acceptable commands before penalty
    pub max_commands: Option<usize>,

    /// Optimal number of commands (good agent behavior)
    pub optimal_commands: Option<usize>,

    /// Point penalties
    pub penalties: PenaltyConfig,

    /// Point bonuses
    pub bonuses: BonusConfig,

    /// Base score (default: 100)
    pub base_score: i32,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            min_commands: None,
            max_commands: None,
            optimal_commands: None,
            penalties: PenaltyConfig::default(),
            bonuses: BonusConfig::default(),
            base_score: 100,
        }
    }
}

/// Penalty point deductions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PenaltyConfig {
    /// Points deducted per command over max_commands (default: -5)
    pub extra_command: i32,

    /// Points deducted for redundant fetches (same resource twice) (default: -10)
    pub redundant_fetch: i32,

    /// Points deducted for unnecessary list operations (default: 0)
    pub unnecessary_list: i32,

    /// Points deducted for failed commands (default: -15)
    pub command_error: i32,
}

impl Default for PenaltyConfig {
    fn default() -> Self {
        Self {
            extra_command: -5,
            redundant_fetch: -10,
            unnecessary_list: 0,
            command_error: -15,
        }
    }
}

/// Bonus point additions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BonusConfig {
    /// Points added for using cache effectively (default: 10)
    pub cache_use: i32,

    /// Points added for completing under optimal commands (default: 0)
    pub under_optimal: i32,

    /// Points added for using JSON output mode (default: 0)
    pub json_output: i32,
}

impl Default for BonusConfig {
    fn default() -> Self {
        Self {
            cache_use: 10,
            under_optimal: 0,
            json_output: 0,
        }
    }
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

    #[test]
    fn test_omitted_scoring_uses_documented_defaults() {
        let meta = r#"
[scenario]
name = "defaults"
description = "no explicit scoring"

[setup]
prompt = "test"
"#;
        let no_scoring: Scenario = toml::from_str(meta).unwrap();
        let partial_scoring: Scenario =
            toml::from_str(&format!("{meta}\n[scoring]\nmax_commands = 5\n")).unwrap();

        for scoring in [no_scoring.scoring, partial_scoring.scoring] {
            assert_eq!(scoring.base_score, 100);
            assert_eq!(scoring.penalties.extra_command, -5);
            assert_eq!(scoring.penalties.redundant_fetch, -10);
            assert_eq!(scoring.penalties.command_error, -15);
            assert_eq!(scoring.bonuses.cache_use, 10);
        }
    }
}
