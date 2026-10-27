//! Agent Harness - Evaluate AI agents using track CLI scenarios
//!
//! This tool runs an AI agent (via Anthropic API or Claude Code CLI) against
//! mock scenarios and evaluates how efficiently and correctly they use the track CLI.

mod anthropic;
mod claude_code;
mod copilot_cli;
mod gemini_runner;
mod permissions;
mod runner;
mod subprocess;
mod text;
mod tools;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use std::path::{Path, PathBuf};
use tracker_mock::{Evaluator, MockClient, Scenario};

/// Model for the Anthropic provider when `--model` is not given
const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";

#[derive(Parser, Debug)]
#[command(name = "track-agent", version, about = "AI agent evaluation harness")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run an agent against a scenario
    Run {
        /// Path to scenario directory
        #[arg(required = true)]
        scenario: PathBuf,

        /// LLM provider to use
        #[arg(long, value_enum, default_value_t = Provider::Anthropic)]
        provider: Provider,

        /// Model to use (default: claude-sonnet-4-20250514)
        #[arg(long)]
        model: Option<String>,

        /// Maximum turns (tool use rounds) before stopping
        #[arg(long, default_value_t = 20)]
        max_turns: usize,

        /// Show verbose output (all messages)
        #[arg(long, short = 'v')]
        verbose: bool,

        /// Output format
        #[arg(long, short = 'o', value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,

        /// Anthropic API key (or set ANTHROPIC_API_KEY env var)
        #[arg(long, env = "ANTHROPIC_API_KEY")]
        api_key: Option<String>,

        /// Minimum score to consider a pass
        #[arg(long, default_value_t = 70)]
        min_score: u8,
    },

    /// Run all scenarios in a directory
    RunAll {
        /// Path to scenarios directory
        #[arg(long, default_value = "./fixtures/scenarios")]
        path: PathBuf,

        /// LLM provider to use
        #[arg(long, value_enum, default_value_t = Provider::Anthropic)]
        provider: Provider,

        /// Model to use
        #[arg(long)]
        model: Option<String>,

        /// Maximum turns per scenario
        #[arg(long, default_value_t = 20)]
        max_turns: usize,

        /// Anthropic API key
        #[arg(long, env = "ANTHROPIC_API_KEY")]
        api_key: Option<String>,

        /// Minimum score to consider a pass
        #[arg(long, default_value_t = 70)]
        min_score: u8,

        /// Stop on first failure
        #[arg(long)]
        fail_fast: bool,

        /// Output format
        #[arg(long, short = 'o', value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },

    /// List available scenarios
    List {
        /// Path to scenarios directory
        #[arg(long, default_value = "./fixtures/scenarios")]
        path: PathBuf,
    },

    /// Show scenario details
    Show {
        /// Path to scenario directory
        #[arg(required = true)]
        scenario: PathBuf,
    },
}

#[derive(ValueEnum, Clone, Debug, Copy, Default)]
enum Provider {
    #[default]
    Anthropic,
    /// Use Claude Code CLI as the agent
    ClaudeCode,
    /// Use GitHub Copilot CLI as the agent
    CopilotCli,
    /// Use Gemini CLI as the agent
    Gemini,
}

#[derive(ValueEnum, Clone, Debug, Copy, Default)]
enum OutputFormat {
    #[default]
    Text,
    Json,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            scenario,
            provider,
            model,
            max_turns,
            verbose,
            format,
            api_key,
            min_score,
        } => run_scenario(
            &scenario, provider, model, max_turns, verbose, format, api_key, min_score,
        ),

        Commands::RunAll {
            path,
            provider,
            model,
            max_turns,
            api_key,
            min_score,
            fail_fast,
            format,
        } => run_all_scenarios(
            &path, provider, model, max_turns, api_key, min_score, fail_fast, format,
        ),

        Commands::List { path } => list_scenarios(&path),

        Commands::Show { scenario } => show_scenario(&scenario),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_scenario(
    scenario_path: &PathBuf,
    provider: Provider,
    model: Option<String>,
    max_turns: usize,
    verbose: bool,
    format: OutputFormat,
    api_key: Option<String>,
    min_score: u8,
) -> Result<()> {
    // Load scenario
    let scenario = Scenario::load_from_dir(scenario_path)?;

    // Clear call log
    let log_path = scenario_path.join("call_log.jsonl");
    std::fs::write(&log_path, "")?;

    let session_result = run_agent_session(
        provider,
        scenario_path,
        &scenario,
        model.as_deref(),
        max_turns,
        verbose,
        api_key.as_deref(),
    )?;

    // Evaluate results
    let client = MockClient::new(scenario_path)?;
    let calls = client.read_call_log()?;
    let evaluator = Evaluator::new(scenario);
    let eval_result = evaluator.evaluate(&calls);
    let passed = eval_result.score_percent >= min_score as f64 && eval_result.success;

    // Output results
    match format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "scenario": eval_result.scenario_name,
                "success": eval_result.success,
                "score": eval_result.score,
                "score_percent": eval_result.score_percent,
                "total_calls": eval_result.total_calls,
                "turns_used": session_result.turns_used,
                "input_tokens": session_result.total_input_tokens,
                "output_tokens": session_result.total_output_tokens,
                "commands_executed": session_result.commands_executed.iter().map(|c| {
                    serde_json::json!({
                        "args": c.args,
                        "output": c.output,
                        "is_error": c.is_error,
                    })
                }).collect::<Vec<_>>(),
                "outcomes": eval_result.outcomes,
                "efficiency": format!("{:?}", eval_result.efficiency),
                "passed": passed,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Text => {
            println!("\n{}", "═".repeat(60));
            println!("{}", "EVALUATION RESULTS".white().bold());
            println!("{}", "═".repeat(60));

            let status = if passed {
                "PASS".green().bold()
            } else {
                "FAIL".red().bold()
            };

            println!(
                "\n{}: {} - {}",
                "Scenario".white().bold(),
                eval_result.scenario_name.cyan(),
                status
            );

            println!(
                "{}: {}/{} ({:.0}%)",
                "Score".white().bold(),
                eval_result.score,
                eval_result.max_score,
                eval_result.score_percent
            );

            println!(
                "{}: {} (optimal: {})",
                "Commands".white().bold(),
                eval_result.total_calls,
                eval_result
                    .optimal_calls
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            );

            println!(
                "{}: {}",
                "Turns used".white().bold(),
                session_result.turns_used
            );

            println!(
                "{}: {:?}",
                "Efficiency".white().bold(),
                eval_result.efficiency
            );

            // Outcomes
            println!("\n{}:", "Expected Outcomes".white().bold());
            for outcome in &eval_result.outcomes {
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

            // Suggestions
            if !eval_result.suggestions.is_empty() {
                println!("\n{}:", "Suggestions".white().bold());
                for suggestion in &eval_result.suggestions {
                    println!("  • {}", suggestion.yellow());
                }
            }

            println!();
        }
    }

    // Exit with error if failed
    if !passed {
        std::process::exit(1);
    }

    Ok(())
}

/// Run one agent session against `scenario` with the selected provider
fn run_agent_session(
    provider: Provider,
    scenario_path: &Path,
    scenario: &Scenario,
    model: Option<&str>,
    max_turns: usize,
    verbose: bool,
    api_key: Option<&str>,
) -> Result<runner::SessionResult> {
    let scenario_path = scenario_path.to_path_buf();
    let scenario = scenario.clone();

    match provider {
        Provider::Anthropic => {
            let api_key =
                api_key.context("API key required. Set ANTHROPIC_API_KEY or use --api-key")?;
            runner::SessionRunner::new(
                scenario_path,
                scenario,
                api_key.to_string(),
                model.unwrap_or(DEFAULT_ANTHROPIC_MODEL).to_string(),
                max_turns,
                verbose,
            )
            .run()
        }
        Provider::ClaudeCode => claude_code::run_claude_code(&claude_code::ClaudeCodeConfig {
            scenario_path,
            scenario,
            max_turns,
            verbose,
        }),
        Provider::CopilotCli => copilot_cli::run_copilot_cli(&copilot_cli::CopilotCliConfig {
            scenario_path,
            scenario,
            max_turns,
            verbose,
            copilot_mode: copilot_cli::CopilotMode::Suggest,
        }),
        Provider::Gemini => gemini_runner::run_gemini(&gemini_runner::GeminiRunnerConfig {
            scenario_path,
            scenario,
            max_turns,
            verbose,
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_all_scenarios(
    path: &PathBuf,
    provider: Provider,
    model: Option<String>,
    max_turns: usize,
    api_key: Option<String>,
    min_score: u8,
    fail_fast: bool,
    format: OutputFormat,
) -> Result<()> {
    // Validate API key for Anthropic provider
    if matches!(provider, Provider::Anthropic) && api_key.is_none() {
        return Err(anyhow::anyhow!(
            "API key required for Anthropic provider. Set ANTHROPIC_API_KEY or use --api-key"
        ));
    }

    let entries = std::fs::read_dir(path)?;
    let mut scenarios: Vec<(PathBuf, Scenario)> = Vec::new();

    for entry in entries.flatten() {
        let scenario_file = entry.path().join("scenario.toml");
        if scenario_file.exists()
            && let Ok(scenario) = Scenario::load(&scenario_file)
        {
            scenarios.push((entry.path(), scenario));
        }
    }

    if scenarios.is_empty() {
        return Err(anyhow::anyhow!("No scenarios found in {}", path.display()));
    }

    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut all_passed = true;

    for (scenario_path, scenario) in &scenarios {
        if matches!(format, OutputFormat::Text) {
            println!(
                "\n{} {}...",
                "Running:".cyan(),
                scenario.scenario.name.white().bold()
            );
        }

        // Clear call log
        let log_path = scenario_path.join("call_log.jsonl");
        std::fs::write(&log_path, "")?;

        let session_result = match run_agent_session(
            provider,
            scenario_path,
            scenario,
            model.as_deref(),
            max_turns,
            false, // not verbose in batch mode
            api_key.as_deref(),
        ) {
            Ok(r) => r,
            Err(e) => {
                // `:#` includes the cause, e.g. why a CLI failed to spawn.
                eprintln!("  {} {:#}", "Error:".red(), e);
                all_passed = false;
                if fail_fast {
                    break;
                }
                continue;
            }
        };

        // Evaluate
        let client = MockClient::new(scenario_path)?;
        let calls = client.read_call_log()?;
        let evaluator = Evaluator::new(scenario.clone());
        let eval_result = evaluator.evaluate(&calls);

        let passed = eval_result.score_percent >= min_score as f64 && eval_result.success;
        if !passed {
            all_passed = false;
        }

        results.push(serde_json::json!({
            "scenario": eval_result.scenario_name,
            "passed": passed,
            "score": eval_result.score,
            "score_percent": eval_result.score_percent,
            "total_calls": eval_result.total_calls,
            "turns_used": session_result.turns_used,
            "input_tokens": session_result.total_input_tokens,
            "output_tokens": session_result.total_output_tokens,
            "commands_executed_count": session_result.commands_executed.len(),
            "success": eval_result.success,
        }));

        if matches!(format, OutputFormat::Text) {
            let status = if passed { "PASS".green() } else { "FAIL".red() };
            println!(
                "  {} - {:.0}% ({} calls, {} turns)",
                status,
                eval_result.score_percent,
                eval_result.total_calls,
                session_result.turns_used
            );
        }

        if !passed && fail_fast {
            break;
        }
    }

    // Final summary
    match format {
        OutputFormat::Json => {
            let passed_count = results
                .iter()
                .filter(|r| r["passed"].as_bool().unwrap_or(false))
                .count();
            let output = serde_json::json!({
                "all_passed": all_passed,
                "total": results.len(),
                "passed": passed_count,
                "failed": results.len() - passed_count,
                "results": results,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Text => {
            println!("\n{}", "─".repeat(60));
            let passed_count = results
                .iter()
                .filter(|r| r["passed"].as_bool().unwrap_or(false))
                .count();
            if all_passed {
                println!(
                    "  {} {}/{} scenarios passed",
                    "✓".green().bold(),
                    passed_count,
                    results.len()
                );
            } else {
                println!(
                    "  {} {}/{} scenarios passed",
                    "✗".red().bold(),
                    passed_count,
                    results.len()
                );
            }
            println!();
        }
    }

    if !all_passed {
        std::process::exit(1);
    }

    Ok(())
}

fn list_scenarios(path: &PathBuf) -> Result<()> {
    let entries = std::fs::read_dir(path)?;

    println!("\n{}:", "Available Scenarios".white().bold());

    for entry in entries.flatten() {
        let scenario_file = entry.path().join("scenario.toml");
        if scenario_file.exists()
            && let Ok(scenario) = Scenario::load(&scenario_file)
        {
            println!(
                "\n  {} ({})",
                scenario.scenario.name.cyan().bold(),
                scenario.scenario.difficulty.dimmed()
            );
            println!("    {}", scenario.scenario.description);
            println!("    Path: {}", entry.path().display().to_string().dimmed());
        }
    }
    println!();
    Ok(())
}

fn show_scenario(scenario_path: &Path) -> Result<()> {
    let scenario = Scenario::load_from_dir(scenario_path)?;

    println!(
        "\n{}: {}",
        "Scenario".white().bold(),
        scenario.scenario.name.cyan().bold()
    );
    println!("{}", "═".repeat(60));
    println!("{}", scenario.scenario.description);

    println!("\n{}:", "Agent Prompt".white().bold());
    println!("{}", "─".repeat(60));
    for line in scenario.setup.prompt.lines() {
        println!("{}", line);
    }
    println!("{}", "─".repeat(60));

    if let Some(context) = &scenario.setup.context {
        println!("\n{}:", "Additional Context".white().bold());
        for line in context.lines() {
            println!("  {}", line.dimmed());
        }
    }

    println!("\n{}:", "Expected Outcomes".white().bold());
    for name in scenario.expected_outcomes.keys() {
        println!("  • {}", name);
    }

    println!("\n{}:", "Scoring".white().bold());
    if let Some(opt) = scenario.scoring.optimal_commands {
        println!("  Optimal commands: {}", opt.to_string().green());
    }
    if let Some(max) = scenario.scoring.max_commands {
        println!("  Max commands: {}", max);
    }
    println!();

    Ok(())
}
