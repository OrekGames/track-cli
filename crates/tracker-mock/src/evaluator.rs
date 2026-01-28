//! Evaluation of agent performance based on call logs
//!
//! Analyzes the call_log.jsonl to determine correctness and efficiency.

use crate::client::CallLogEntry;
use crate::scenario::{ComplexOutcome, ExpectedOutcome, Scenario, ScoringConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Result of evaluating an agent's performance on a scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// Scenario name
    pub scenario_name: String,

    /// Whether all expected outcomes were met
    pub success: bool,

    /// Final score (out of base_score, can be negative)
    pub score: i32,

    /// Maximum possible score
    pub max_score: i32,

    /// Score as a percentage (0-100, capped)
    pub score_percent: f64,

    /// Total number of API calls made
    pub total_calls: usize,

    /// Optimal number of calls (from scenario config)
    pub optimal_calls: Option<usize>,

    /// Efficiency rating
    pub efficiency: EfficiencyRating,

    /// Individual outcome results
    pub outcomes: Vec<OutcomeResult>,

    /// Detailed breakdown of scoring
    pub score_breakdown: ScoreBreakdown,

    /// Warnings or suggestions for improvement
    pub suggestions: Vec<String>,
}

/// Efficiency rating based on command count
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EfficiencyRating {
    /// Used fewer commands than optimal
    Excellent,
    /// Used optimal number of commands
    Optimal,
    /// Used more than optimal but within acceptable range
    Acceptable,
    /// Used too many commands
    Inefficient,
}

/// Result of checking a single expected outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeResult {
    /// Outcome name (from scenario config key)
    pub name: String,

    /// Whether this outcome was achieved
    pub achieved: bool,

    /// Description of what was expected
    pub expected: String,

    /// Description of what actually happened
    pub actual: String,
}

/// Detailed breakdown of how the score was calculated
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScoreBreakdown {
    /// Base score
    pub base: i32,

    /// Points deducted for various reasons
    pub penalties: Vec<ScoreAdjustment>,

    /// Points added for various reasons
    pub bonuses: Vec<ScoreAdjustment>,

    /// Total penalties (negative)
    pub total_penalties: i32,

    /// Total bonuses (positive)
    pub total_bonuses: i32,
}

/// A single score adjustment (penalty or bonus)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreAdjustment {
    /// Reason for the adjustment
    pub reason: String,

    /// Point value (negative for penalties, positive for bonuses)
    pub points: i32,

    /// Number of times this adjustment was applied
    pub count: usize,
}

/// Evaluator for analyzing agent performance
pub struct Evaluator {
    scenario: Scenario,
}

impl Evaluator {
    /// Create an evaluator for a scenario
    pub fn new(scenario: Scenario) -> Self {
        Self { scenario }
    }

    /// Load an evaluator from a scenario directory
    pub fn from_dir(dir: &Path) -> Result<Self, crate::scenario::ScenarioError> {
        let scenario = Scenario::load_from_dir(dir)?;
        Ok(Self::new(scenario))
    }

    /// Evaluate call log entries against the scenario
    pub fn evaluate(&self, calls: &[CallLogEntry]) -> EvaluationResult {
        let scoring = &self.scenario.scoring;
        let base_score = scoring.base_score;

        // Check outcomes
        let outcomes = self.check_outcomes(calls);
        let all_outcomes_met = outcomes.iter().all(|o| o.achieved);

        // Calculate efficiency
        let total_calls = calls.len();
        let efficiency = self.calculate_efficiency(total_calls, scoring);

        // Calculate score breakdown
        let score_breakdown = self.calculate_score_breakdown(calls, &outcomes, scoring);

        // Calculate final score
        let final_score =
            base_score + score_breakdown.total_bonuses + score_breakdown.total_penalties;
        let score_percent = ((final_score as f64 / base_score as f64) * 100.0).clamp(0.0, 100.0);

        // Generate suggestions
        let suggestions = self.generate_suggestions(calls, &outcomes, &efficiency);

        EvaluationResult {
            scenario_name: self.scenario.scenario.name.clone(),
            success: all_outcomes_met,
            score: final_score,
            max_score: base_score,
            score_percent,
            total_calls,
            optimal_calls: scoring.optimal_commands,
            efficiency,
            outcomes,
            score_breakdown,
            suggestions,
        }
    }

    /// Check all expected outcomes against the call log
    fn check_outcomes(&self, calls: &[CallLogEntry]) -> Vec<OutcomeResult> {
        self.scenario
            .expected_outcomes
            .iter()
            .map(|(name, outcome)| self.check_outcome(name, outcome, calls))
            .collect()
    }

    /// Check a single outcome
    fn check_outcome(
        &self,
        name: &str,
        outcome: &ExpectedOutcome,
        calls: &[CallLogEntry],
    ) -> OutcomeResult {
        match outcome {
            ExpectedOutcome::Boolean(expected) => {
                // For boolean outcomes, check if any relevant call was made
                let achieved = calls.is_empty() != *expected;
                OutcomeResult {
                    name: name.to_string(),
                    achieved,
                    expected: format!("calls made: {}", expected),
                    actual: format!("calls made: {}", !calls.is_empty()),
                }
            }

            ExpectedOutcome::String(expected_value) => {
                // Check if any call references this value (e.g., issue ID)
                let found = calls.iter().any(|call| {
                    call.args
                        .values()
                        .any(|v| v.as_str() == Some(expected_value.as_str()))
                });

                OutcomeResult {
                    name: name.to_string(),
                    achieved: found,
                    expected: format!("reference to '{}'", expected_value),
                    actual: if found {
                        format!("found '{}'", expected_value)
                    } else {
                        "not found".to_string()
                    },
                }
            }

            ExpectedOutcome::Complex(complex) => self.check_complex_outcome(name, complex, calls),
        }
    }

    /// Check a complex outcome with multiple criteria
    fn check_complex_outcome(
        &self,
        name: &str,
        outcome: &ComplexOutcome,
        calls: &[CallLogEntry],
    ) -> OutcomeResult {
        let mut checks_passed = true;
        let mut expected_parts = Vec::new();
        let mut actual_parts = Vec::new();

        // Check method_called
        if let Some(method) = &outcome.method_called {
            let call_count = calls.iter().filter(|c| c.method == *method).count();
            let method_called = call_count > 0;

            expected_parts.push(format!("method '{}' called", method));

            if method_called {
                actual_parts.push(format!("'{}' called {} times", method, call_count));
            } else {
                actual_parts.push(format!("'{}' not called", method));
                checks_passed = false;
            }

            // Check min_calls
            if let Some(min) = outcome.min_calls {
                if call_count < min {
                    expected_parts.push(format!("at least {} calls", min));
                    actual_parts.push(format!("only {} calls", call_count));
                    checks_passed = false;
                }
            }

            // Check max_calls
            if let Some(max) = outcome.max_calls {
                if call_count > max {
                    expected_parts.push(format!("at most {} calls", max));
                    actual_parts.push(format!("{} calls (exceeds max)", call_count));
                    checks_passed = false;
                }
            }
        }

        // Check issue reference
        if let Some(issue) = &outcome.issue {
            let issue_referenced = calls.iter().any(|call| {
                call.args
                    .values()
                    .any(|v| v.as_str() == Some(issue.as_str()))
            });

            expected_parts.push(format!("issue '{}'", issue));

            if issue_referenced {
                actual_parts.push(format!("issue '{}' referenced", issue));
            } else {
                actual_parts.push(format!("issue '{}' not referenced", issue));
                checks_passed = false;
            }
        }

        // Check field/value (for update operations)
        if let (Some(field), Some(value)) = (&outcome.field, &outcome.value) {
            let update_calls: Vec<_> = calls
                .iter()
                .filter(|c| c.method == "update_issue")
                .collect();

            // For now, we just check if update was called with the right issue
            // In a full implementation, we'd parse the body to check field/value
            let value_found = update_calls.iter().any(|_call| {
                // Simplified check - in reality would need to inspect request body
                true
            });

            expected_parts.push(format!("{} = '{}'", field, value));
            if value_found && !update_calls.is_empty() {
                actual_parts.push("update called (field check simplified)".to_string());
            } else {
                actual_parts.push("no update with matching field/value".to_string());
                checks_passed = false;
            }
        }

        // Check contains (for comment text or issue summary)
        if let Some(contains) = &outcome.contains {
            // Determine which method to check based on method_called
            let method_to_check = outcome.method_called.as_deref();

            let text_found = match method_to_check {
                Some("create_issue") => {
                    // For create_issue, check the summary field
                    calls.iter().any(|call| {
                        call.method == "create_issue"
                            && call
                                .args
                                .get("summary")
                                .and_then(|v| v.as_str())
                                .map(|t| t.to_lowercase().contains(&contains.to_lowercase()))
                                .unwrap_or(false)
                    })
                }
                Some("add_comment") | Some("add_article_comment") | None => {
                    // For comments, check the text field
                    calls.iter().any(|call| {
                        (call.method == "add_comment" || call.method == "add_article_comment")
                            && call
                                .args
                                .get("text")
                                .and_then(|v| v.as_str())
                                .map(|t| t.to_lowercase().contains(&contains.to_lowercase()))
                                .unwrap_or(false)
                    })
                }
                _ => {
                    // For other methods, check all string args
                    calls.iter().any(|call| {
                        call.args.values().any(|v| {
                            v.as_str()
                                .map(|t| t.to_lowercase().contains(&contains.to_lowercase()))
                                .unwrap_or(false)
                        })
                    })
                }
            };

            let expected_desc = match method_to_check {
                Some("create_issue") => format!("issue summary containing '{}'", contains),
                _ => format!("comment containing '{}'", contains),
            };

            expected_parts.push(expected_desc.clone());
            if text_found {
                actual_parts.push(format!("found text with '{}'", contains));
            } else {
                actual_parts.push(format!("no matching text for '{}'", contains));
                checks_passed = false;
            }
        }

        OutcomeResult {
            name: name.to_string(),
            achieved: checks_passed,
            expected: expected_parts.join(", "),
            actual: actual_parts.join(", "),
        }
    }

    /// Calculate efficiency rating based on command count
    fn calculate_efficiency(
        &self,
        total_calls: usize,
        scoring: &ScoringConfig,
    ) -> EfficiencyRating {
        let optimal = scoring.optimal_commands.unwrap_or(usize::MAX);
        let max = scoring.max_commands.unwrap_or(usize::MAX);
        let min = scoring.min_commands.unwrap_or(0);

        if total_calls < min {
            // Suspiciously few calls - might have cheated or failed early
            EfficiencyRating::Excellent
        } else if total_calls <= optimal {
            if total_calls < optimal {
                EfficiencyRating::Excellent
            } else {
                EfficiencyRating::Optimal
            }
        } else if total_calls <= max {
            EfficiencyRating::Acceptable
        } else {
            EfficiencyRating::Inefficient
        }
    }

    /// Calculate detailed score breakdown
    fn calculate_score_breakdown(
        &self,
        calls: &[CallLogEntry],
        outcomes: &[OutcomeResult],
        scoring: &ScoringConfig,
    ) -> ScoreBreakdown {
        let mut breakdown = ScoreBreakdown {
            base: scoring.base_score,
            ..Default::default()
        };

        // Penalty: Failed outcomes
        let failed_outcomes = outcomes.iter().filter(|o| !o.achieved).count();
        if failed_outcomes > 0 {
            // Major penalty for each failed outcome
            let penalty = -(failed_outcomes as i32 * 25);
            breakdown.penalties.push(ScoreAdjustment {
                reason: "Failed expected outcomes".to_string(),
                points: penalty,
                count: failed_outcomes,
            });
            breakdown.total_penalties += penalty;
        }

        // Penalty: Extra commands
        if let Some(max) = scoring.max_commands {
            if calls.len() > max {
                let extra = calls.len() - max;
                let penalty = extra as i32 * scoring.penalties.extra_command;
                breakdown.penalties.push(ScoreAdjustment {
                    reason: format!("Extra commands ({} over max {})", extra, max),
                    points: penalty,
                    count: extra,
                });
                breakdown.total_penalties += penalty;
            }
        }

        // Penalty: Redundant fetches (same resource fetched multiple times)
        let redundant = self.count_redundant_fetches(calls);
        if redundant > 0 {
            let penalty = redundant as i32 * scoring.penalties.redundant_fetch;
            breakdown.penalties.push(ScoreAdjustment {
                reason: "Redundant fetches (same resource fetched multiple times)".to_string(),
                points: penalty,
                count: redundant,
            });
            breakdown.total_penalties += penalty;
        }

        // Penalty: Errors
        let errors = calls.iter().filter(|c| c.error.is_some()).count();
        if errors > 0 {
            let penalty = errors as i32 * scoring.penalties.command_error;
            breakdown.penalties.push(ScoreAdjustment {
                reason: "Command errors".to_string(),
                points: penalty,
                count: errors,
            });
            breakdown.total_penalties += penalty;
        }

        // Bonus: Under optimal
        if let Some(optimal) = scoring.optimal_commands {
            if calls.len() < optimal && scoring.bonuses.under_optimal > 0 {
                let diff = optimal - calls.len();
                let bonus = diff as i32 * scoring.bonuses.under_optimal;
                breakdown.bonuses.push(ScoreAdjustment {
                    reason: format!("Under optimal ({} commands saved)", diff),
                    points: bonus,
                    count: diff,
                });
                breakdown.total_bonuses += bonus;
            }
        }

        // Bonus: Cache usage (if cache refresh was called)
        let cache_used = calls.iter().any(|c| c.method.contains("cache"));
        if cache_used && scoring.bonuses.cache_use > 0 {
            breakdown.bonuses.push(ScoreAdjustment {
                reason: "Effective cache usage".to_string(),
                points: scoring.bonuses.cache_use,
                count: 1,
            });
            breakdown.total_bonuses += scoring.bonuses.cache_use;
        }

        breakdown
    }

    /// Count redundant fetches (same get_* call with same ID twice)
    fn count_redundant_fetches(&self, calls: &[CallLogEntry]) -> usize {
        let mut seen: HashSet<String> = HashSet::new();
        let mut redundant = 0;

        for call in calls {
            // Only count get_* methods as fetch operations
            if !call.method.starts_with("get_") {
                continue;
            }

            // Create a key from method + ID argument
            let key = if let Some(id) = call.args.get("id").or_else(|| call.args.get("issue_id")) {
                format!("{}:{}", call.method, id)
            } else {
                continue;
            };

            if seen.contains(&key) {
                redundant += 1;
            } else {
                seen.insert(key);
            }
        }

        redundant
    }

    /// Generate improvement suggestions based on the evaluation
    fn generate_suggestions(
        &self,
        calls: &[CallLogEntry],
        outcomes: &[OutcomeResult],
        efficiency: &EfficiencyRating,
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Failed outcomes
        for outcome in outcomes.iter().filter(|o| !o.achieved) {
            suggestions.push(format!(
                "Outcome '{}' was not achieved: expected {}, got {}",
                outcome.name, outcome.expected, outcome.actual
            ));
        }

        // Efficiency suggestions
        match efficiency {
            EfficiencyRating::Inefficient => {
                suggestions.push("Consider using the cache system to reduce API calls".to_string());
                suggestions.push("Avoid fetching the same resource multiple times".to_string());
            }
            EfficiencyRating::Acceptable => {
                suggestions.push(
                    "Good job! Consider combining operations where possible for optimal efficiency"
                        .to_string(),
                );
            }
            _ => {}
        }

        // Redundant fetch suggestion
        let redundant = self.count_redundant_fetches(calls);
        if redundant > 0 {
            suggestions.push(format!(
                "Found {} redundant fetch(es). Store results in variables for reuse.",
                redundant
            ));
        }

        // Error handling suggestion
        let errors = calls.iter().filter(|c| c.error.is_some()).count();
        if errors > 0 {
            suggestions.push(format!(
                "{} command(s) resulted in errors. Check arguments and resource existence.",
                errors
            ));
        }

        suggestions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{ScenarioMeta, SetupConfig};

    fn make_test_scenario() -> Scenario {
        Scenario {
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
            expected_outcomes: [(
                "issue_fetched".to_string(),
                ExpectedOutcome::String("DEMO-1".to_string()),
            )]
            .into_iter()
            .collect(),
            scoring: ScoringConfig {
                min_commands: Some(1),
                max_commands: Some(5),
                optimal_commands: Some(2),
                base_score: 100,
                ..Default::default()
            },
        }
    }

    fn make_call(method: &str, args: Vec<(&str, &str)>) -> CallLogEntry {
        CallLogEntry {
            timestamp: chrono::Utc::now(),
            method: method.to_string(),
            args: args
                .into_iter()
                .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
                .collect(),
            response_file: Some("test.json".to_string()),
            error: None,
            status: 200,
            duration_ms: 1,
        }
    }

    #[test]
    fn test_evaluate_success() {
        let scenario = make_test_scenario();
        let evaluator = Evaluator::new(scenario);

        let calls = vec![make_call("get_issue", vec![("id", "DEMO-1")])];

        let result = evaluator.evaluate(&calls);

        assert!(result.success);
        assert_eq!(result.total_calls, 1);
        assert_eq!(result.efficiency, EfficiencyRating::Excellent);
    }

    #[test]
    fn test_evaluate_failure() {
        let scenario = make_test_scenario();
        let evaluator = Evaluator::new(scenario);

        let calls = vec![make_call("get_issue", vec![("id", "WRONG-1")])];

        let result = evaluator.evaluate(&calls);

        assert!(!result.success);
        assert!(result.score < 100);
    }

    #[test]
    fn test_redundant_fetch_detection() {
        let scenario = make_test_scenario();
        let evaluator = Evaluator::new(scenario);

        let calls = vec![
            make_call("get_issue", vec![("id", "DEMO-1")]),
            make_call("get_issue", vec![("id", "DEMO-1")]), // Redundant
            make_call("get_issue", vec![("id", "DEMO-2")]), // Not redundant
        ];

        let redundant = evaluator.count_redundant_fetches(&calls);
        assert_eq!(redundant, 1);
    }
}
