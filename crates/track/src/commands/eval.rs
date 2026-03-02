use anyhow::Result;

use crate::cli;
use crate::output::output_json;

/// Print evaluation result in the specified format
fn print_eval_result(
    result: &tracker_mock::EvaluationResult,
    format: cli::OutputFormat,
) -> Result<()> {
    match format {
        cli::OutputFormat::Json => {
            output_json(result)?;
        }
        cli::OutputFormat::Text => {
            use colored::Colorize;

            // Header
            println!(
                "\n{}: {}",
                "Scenario".white().bold(),
                result.scenario_name.cyan()
            );
            println!("{}", "=".repeat(60));

            // Overall result
            let status = if result.success {
                "PASS".green().bold()
            } else {
                "FAIL".red().bold()
            };
            println!("\n{}: {}", "Result".white().bold(), status);
            println!(
                "{}: {}/{} ({:.0}%)",
                "Score".white().bold(),
                result.score,
                result.max_score,
                result.score_percent
            );
            println!(
                "{}: {} (optimal: {})",
                "Commands".white().bold(),
                result.total_calls,
                result
                    .optimal_calls
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            );
            println!("{}: {:?}", "Efficiency".white().bold(), result.efficiency);

            // Outcomes
            println!("\n{}:", "Expected Outcomes".white().bold());
            for outcome in &result.outcomes {
                let icon = if outcome.achieved {
                    "✓".green()
                } else {
                    "✗".red()
                };
                println!("  {} {}", icon, outcome.name);
                if !outcome.achieved {
                    println!("    Expected: {}", outcome.expected.dimmed());
                    println!("    Actual:   {}", outcome.actual.yellow());
                }
            }

            // Score breakdown
            if !result.score_breakdown.penalties.is_empty()
                || !result.score_breakdown.bonuses.is_empty()
            {
                println!("\n{}:", "Score Breakdown".white().bold());
                println!("  Base: {}", result.score_breakdown.base);

                for bonus in &result.score_breakdown.bonuses {
                    println!(
                        "  {} {} (x{})",
                        format!("+{}", bonus.points).green(),
                        bonus.reason,
                        bonus.count
                    );
                }

                for penalty in &result.score_breakdown.penalties {
                    println!(
                        "  {} {} (x{})",
                        penalty.points.to_string().red(),
                        penalty.reason,
                        penalty.count
                    );
                }
            }

            // Suggestions
            if !result.suggestions.is_empty() {
                println!("\n{}:", "Suggestions".white().bold());
                for suggestion in &result.suggestions {
                    println!("  • {}", suggestion.yellow());
                }
            }

            println!();
        }
    }
    Ok(())
}

pub fn handle_eval(action: &cli::EvalCommands, format: cli::OutputFormat) -> Result<()> {
    use cli::EvalCommands;
    use tracker_mock::{EvaluationResult, Evaluator, MockClient, Scenario};

    match action {
        EvalCommands::Run {
            scenario,
            min_score,
            strict,
        } => {
            // Load scenario and call log
            let scenario_data = Scenario::load_from_dir(scenario)
                .map_err(|e| anyhow::anyhow!("Failed to load scenario: {}", e))?;

            let client = MockClient::new(scenario)
                .map_err(|e| anyhow::anyhow!("Failed to load mock client: {}", e))?;

            let calls = client
                .read_call_log()
                .map_err(|e| anyhow::anyhow!("Failed to read call log: {}", e))?;

            if calls.is_empty() {
                return Err(anyhow::anyhow!(
                    "Call log is empty. Run commands with TRACK_MOCK_DIR={} first.",
                    scenario.display()
                ));
            }

            // Run evaluation
            let evaluator = Evaluator::new(scenario_data);
            let result = evaluator.evaluate(&calls);

            // Print results
            print_eval_result(&result, format)?;

            // Check CI thresholds
            let score_ok = result.score_percent >= *min_score as f64;
            let strict_ok = !*strict || result.success;

            if !score_ok {
                return Err(anyhow::anyhow!(
                    "Score {:.0}% is below minimum required {}%",
                    result.score_percent,
                    min_score
                ));
            }

            if !strict_ok {
                return Err(anyhow::anyhow!(
                    "Not all expected outcomes were achieved (--strict mode)"
                ));
            }

            Ok(())
        }

        EvalCommands::RunAll {
            path,
            min_score,
            fail_fast,
        } => {
            let entries = std::fs::read_dir(path)
                .map_err(|e| anyhow::anyhow!("Failed to read scenarios directory: {}", e))?;

            let mut scenarios: Vec<(std::path::PathBuf, Scenario)> = Vec::new();
            for entry in entries.flatten() {
                let scenario_file = entry.path().join("scenario.toml");
                if scenario_file.exists() {
                    if let Ok(scenario) = Scenario::load(&scenario_file) {
                        scenarios.push((entry.path(), scenario));
                    }
                }
            }

            if scenarios.is_empty() {
                return Err(anyhow::anyhow!("No scenarios found in {}", path.display()));
            }

            let mut results: Vec<(String, EvaluationResult, bool)> = Vec::new();
            let mut all_passed = true;

            for (scenario_path, scenario_data) in &scenarios {
                let client = match MockClient::new(scenario_path) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Warning: Failed to load {}: {}", scenario_path.display(), e);
                        continue;
                    }
                };

                let calls = match client.read_call_log() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to read call log for {}: {}",
                            scenario_data.scenario.name, e
                        );
                        continue;
                    }
                };

                if calls.is_empty() {
                    match format {
                        cli::OutputFormat::Json => {}
                        cli::OutputFormat::Text => {
                            use colored::Colorize;
                            println!(
                                "{}: {} - {}",
                                "SKIP".yellow(),
                                scenario_data.scenario.name,
                                "empty call log".dimmed()
                            );
                        }
                    }
                    continue;
                }

                let evaluator = Evaluator::new(scenario_data.clone());
                let result = evaluator.evaluate(&calls);

                let passed = result.score_percent >= *min_score as f64 && result.success;
                if !passed {
                    all_passed = false;
                }

                results.push((scenario_data.scenario.name.clone(), result, passed));

                if !passed && *fail_fast {
                    break;
                }
            }

            // Print summary
            match format {
                cli::OutputFormat::Json => {
                    let summary: Vec<_> = results
                        .iter()
                        .map(|(name, result, passed)| {
                            serde_json::json!({
                                "scenario": name,
                                "passed": passed,
                                "score": result.score,
                                "score_percent": result.score_percent,
                                "total_calls": result.total_calls,
                                "success": result.success,
                            })
                        })
                        .collect();
                    let output = serde_json::json!({
                        "all_passed": all_passed,
                        "total": results.len(),
                        "passed": results.iter().filter(|(_, _, p)| *p).count(),
                        "failed": results.iter().filter(|(_, _, p)| !*p).count(),
                        "results": summary,
                    });
                    output_json(&output)?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    println!("\n{}", "Evaluation Results".white().bold());
                    println!("{}", "=".repeat(60));

                    for (name, result, passed) in &results {
                        let status = if *passed {
                            "PASS".green()
                        } else {
                            "FAIL".red()
                        };
                        println!(
                            "  {} {} - {:.0}% ({} calls)",
                            status,
                            name.cyan(),
                            result.score_percent,
                            result.total_calls
                        );
                    }

                    println!("{}", "-".repeat(60));
                    let passed_count = results.iter().filter(|(_, _, p)| *p).count();
                    let total = results.len();

                    if all_passed {
                        println!(
                            "  {} {}/{} scenarios passed",
                            "✓".green().bold(),
                            passed_count,
                            total
                        );
                    } else {
                        println!(
                            "  {} {}/{} scenarios passed",
                            "✗".red().bold(),
                            passed_count,
                            total
                        );
                    }
                    println!();
                }
            }

            if !all_passed {
                return Err(anyhow::anyhow!("One or more scenarios failed"));
            }

            Ok(())
        }

        EvalCommands::List { path } => {
            let entries = std::fs::read_dir(path)
                .map_err(|e| anyhow::anyhow!("Failed to read scenarios directory: {}", e))?;

            let mut scenarios = Vec::new();
            for entry in entries.flatten() {
                let scenario_file = entry.path().join("scenario.toml");
                if scenario_file.exists() {
                    if let Ok(scenario) = Scenario::load(&scenario_file) {
                        scenarios.push((entry.path(), scenario));
                    }
                }
            }

            match format {
                cli::OutputFormat::Json => {
                    let list: Vec<_> = scenarios
                        .iter()
                        .map(|(path, s)| {
                            serde_json::json!({
                                "path": path,
                                "name": s.scenario.name,
                                "description": s.scenario.description,
                                "backend": s.scenario.backend,
                                "difficulty": s.scenario.difficulty,
                            })
                        })
                        .collect();
                    output_json(&list)?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    if scenarios.is_empty() {
                        println!("No scenarios found in {}", path.display());
                        return Ok(());
                    }

                    println!("{}:", "Available Scenarios".white().bold());
                    for (scenario_path, scenario) in &scenarios {
                        println!(
                            "\n  {} ({})",
                            scenario.scenario.name.cyan().bold(),
                            scenario.scenario.difficulty.dimmed()
                        );
                        println!("    {}", scenario.scenario.description);
                        println!("    Path: {}", scenario_path.display().to_string().dimmed());
                        if !scenario.scenario.tags.is_empty() {
                            println!("    Tags: {}", scenario.scenario.tags.join(", ").magenta());
                        }
                    }
                    println!();
                }
            }
            Ok(())
        }

        EvalCommands::Show { scenario } => {
            let scenario_data = Scenario::load_from_dir(scenario)
                .map_err(|e| anyhow::anyhow!("Failed to load scenario: {}", e))?;

            match format {
                cli::OutputFormat::Json => {
                    // Return the full scenario as JSON
                    output_json(&scenario_data)?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    println!(
                        "\n{}: {}",
                        "Scenario".white().bold(),
                        scenario_data.scenario.name.cyan().bold()
                    );
                    println!("{}", "=".repeat(60));
                    println!("{}", scenario_data.scenario.description);

                    println!("\n{}:", "Setup".white().bold());
                    println!("  Backend: {}", scenario_data.scenario.backend.cyan());
                    println!(
                        "  Difficulty: {}",
                        scenario_data.scenario.difficulty.yellow()
                    );
                    if let Some(project) = &scenario_data.setup.default_project {
                        println!("  Default Project: {}", project.cyan());
                    }
                    if scenario_data.setup.cache_available {
                        println!("  Cache: {}", "available".green());
                    }

                    println!("\n{}:", "Agent Prompt".white().bold());
                    for line in scenario_data.setup.prompt.lines() {
                        println!("  {}", line);
                    }

                    if let Some(context) = &scenario_data.setup.context {
                        println!("\n{}:", "Additional Context".white().bold());
                        for line in context.lines() {
                            println!("  {}", line.dimmed());
                        }
                    }

                    println!("\n{}:", "Expected Outcomes".white().bold());
                    for name in scenario_data.expected_outcomes.keys() {
                        println!("  • {}", name);
                    }

                    println!("\n{}:", "Scoring".white().bold());
                    if let Some(min) = scenario_data.scoring.min_commands {
                        println!("  Min commands: {}", min);
                    }
                    if let Some(opt) = scenario_data.scoring.optimal_commands {
                        println!("  Optimal commands: {}", opt.to_string().green());
                    }
                    if let Some(max) = scenario_data.scoring.max_commands {
                        println!("  Max commands: {}", max);
                    }
                    println!("  Base score: {}", scenario_data.scoring.base_score);

                    println!("\n{}:", "Usage".white().bold());
                    println!(
                        "  1. Clear log: {}",
                        format!("track eval clear {}", scenario.display()).cyan()
                    );
                    println!(
                        "  2. Run agent: {}",
                        format!("TRACK_MOCK_DIR={} <agent commands>", scenario.display()).cyan()
                    );
                    println!(
                        "  3. Evaluate:  {}",
                        format!("track eval run {}", scenario.display()).cyan()
                    );
                    println!();
                }
            }
            Ok(())
        }

        EvalCommands::Clear { scenario } => {
            let log_path = scenario.join("call_log.jsonl");

            if log_path.exists() {
                std::fs::write(&log_path, "")?;
            }

            match format {
                cli::OutputFormat::Json => {
                    output_json(&serde_json::json!({
                        "success": true,
                        "path": log_path.display().to_string()
                    }))?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{} {}", "Cleared:".green(), log_path.display());
                }
            }
            Ok(())
        }

        EvalCommands::ClearAll { path } => {
            let entries = std::fs::read_dir(path)
                .map_err(|e| anyhow::anyhow!("Failed to read scenarios directory: {}", e))?;

            let mut cleared = 0;
            for entry in entries.flatten() {
                let log_path = entry.path().join("call_log.jsonl");
                if log_path.exists() {
                    std::fs::write(&log_path, "")?;
                    cleared += 1;
                }
            }

            match format {
                cli::OutputFormat::Json => {
                    output_json(&serde_json::json!({
                        "success": true,
                        "cleared": cleared
                    }))?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;
                    println!("{} {} scenario call logs", "Cleared:".green(), cleared);
                }
            }
            Ok(())
        }

        EvalCommands::Status => {
            let mock_dir = tracker_mock::get_mock_dir();
            let is_enabled = mock_dir.is_some();

            match format {
                cli::OutputFormat::Json => {
                    let status = serde_json::json!({
                        "mock_enabled": is_enabled,
                        "mock_dir": mock_dir,
                        "env_var": tracker_mock::MOCK_DIR_ENV,
                    });
                    output_json(&status)?;
                }
                cli::OutputFormat::Text => {
                    use colored::Colorize;

                    println!("{}:", "Mock System Status".white().bold());
                    println!(
                        "  Environment variable: {}",
                        tracker_mock::MOCK_DIR_ENV.cyan()
                    );

                    if is_enabled {
                        println!("  Status: {}", "ENABLED".green().bold());
                        println!("  Mock directory: {}", mock_dir.unwrap().display());
                        println!(
                            "\n  {} All track commands will use mock responses.",
                            "Note:".yellow()
                        );
                    } else {
                        println!("  Status: {}", "disabled".dimmed());
                        println!(
                            "\n  To enable: {}",
                            "export TRACK_MOCK_DIR=./fixtures/scenarios/<name>".cyan()
                        );
                    }
                }
            }
            Ok(())
        }
    }
}
