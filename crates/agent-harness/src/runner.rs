//! Session runner - manages the conversation with the LLM
//!
//! Handles the agentic loop: prompt -> response -> tool use -> result -> repeat

use crate::anthropic::{
    Client, ContentBlock, Message, MessageContent, MessageRequest, Role, StopReason,
};
use crate::tools;
use anyhow::Result;
use colored::Colorize;
use std::path::{Path, PathBuf};
use tracker_mock::{ContextUsage, Scenario};

/// Result of running a session
pub struct SessionResult {
    pub turns_used: usize,
    /// `None` when the provider does not report token usage (CLI runners)
    pub total_input_tokens: Option<u32>,
    /// `None` when the provider does not report token usage (CLI runners)
    pub total_output_tokens: Option<u32>,
    pub commands_executed: Vec<CommandExecution>,
    /// Bytes of prompt the agent started with (system prompt plus task)
    pub prompt_bytes: usize,
}

impl SessionResult {
    /// The context the agent consumed: its prompt, and every byte of `track`
    /// output it read across the session
    pub fn context_usage(&self) -> ContextUsage {
        ContextUsage {
            prompt_bytes: self.prompt_bytes,
            tool_output_bytes: self.commands_executed.iter().map(|c| c.output.len()).sum(),
        }
    }
}

/// What reference material the agent gets in its system prompt
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContextMode {
    /// The core skill plus the scenario backend's reference, as
    /// `track agent install` ships them
    #[default]
    Skill,
    /// The output of `track prime`, as a session hook would inject it
    Prime,
    /// Nothing: the agent has only `track --help`
    None,
}

/// What one scenario run gives every provider, prepared once before the agent
/// starts.
#[derive(Clone, Debug, Default)]
pub struct RunSetup {
    /// The reference section of the system prompt, per the context mode
    pub reference: String,
    /// Environment a `track` command of the run gets beyond the mock directory
    pub track_env: Vec<(&'static str, String)>,
}

impl RunSetup {
    /// Prepare a run of `scenario` with `cache_dir` as its private cache.
    ///
    /// In [`ContextMode::Prime`] this runs `track cache refresh` and
    /// `track prime` against the mock, as a project's session hook would have.
    /// Those calls land in the call log, so clear it afterwards: they are not
    /// part of the run being scored.
    pub fn prepare(
        scenario_path: &Path,
        scenario: &Scenario,
        context: ContextMode,
        cache_dir: &Path,
    ) -> Self {
        let track_env = track_env(
            context,
            cache_dir,
            scenario.setup.default_project.as_deref(),
        );
        let reference = match context {
            ContextMode::None => String::new(),
            ContextMode::Skill => skill_reference(&scenario.scenario.backend),
            ContextMode::Prime => {
                // A project that uses the hook has a cache, so build one first.
                let _ = tools::execute_track(
                    &["cache".into(), "refresh".into()],
                    scenario_path,
                    &track_env,
                );
                let (output, is_error) =
                    tools::execute_track(&["prime".into()], scenario_path, &track_env);
                prime_reference(&output, is_error)
            }
        };
        Self {
            reference,
            track_env,
        }
    }
}

/// Environment a `track` command of a run gets beyond the mock directory:
/// the run's private cache, so the agent's own cache or an earlier run cannot
/// change how many backend calls this one makes, and, when the session hook
/// is being simulated, the project default and output mode the hook sets.
fn track_env(
    context: ContextMode,
    cache_dir: &Path,
    default_project: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut env = vec![("TRACK_CACHE_DIR", cache_dir.display().to_string())];
    if context == ContextMode::Prime {
        if let Some(project) = default_project {
            env.push(("TRACKER_DEFAULT_PROJECT", project.to_string()));
        }
        env.push(("TRACK_OUTPUT", "agent".to_string()));
    }
    env
}

/// The reference section for [`ContextMode::Prime`]: what the session hook
/// printed, or nothing if `track prime` failed or printed nothing.
fn prime_reference(output: &str, is_error: bool) -> String {
    if is_error || output.trim().is_empty() {
        return String::new();
    }
    format!(
        "## Project briefing (printed by `track prime` at session start)\n\n```\n{}\n```\n\n\
         `track guide` lists reference topics; `track <command> -h` shows a command's options.\n\n",
        output.trim_end()
    )
}

/// The core skill, as `track agent install` ships it from this checkout
const SKILL_CORE: &str = include_str!("../../../agent-skills/SKILL.md");

/// The reference for `backend`, which the core skill tells an agent to read
/// first. `None` for a scenario that runs against any backend.
fn backend_reference(backend: &str) -> Option<&'static str> {
    match backend.to_ascii_lowercase().as_str() {
        "youtrack" => Some(include_str!("../../../agent-skills/references/youtrack.md")),
        "jira" => Some(include_str!("../../../agent-skills/references/jira.md")),
        "github" => Some(include_str!("../../../agent-skills/references/github.md")),
        "gitlab" => Some(include_str!("../../../agent-skills/references/gitlab.md")),
        "linear" => Some(include_str!("../../../agent-skills/references/linear.md")),
        _ => None,
    }
}

/// The reference section for [`ContextMode::Skill`]: what an agent with the
/// skill installed starts with, the core skill plus the reference for the
/// scenario's backend.
fn skill_reference(backend: &str) -> String {
    let mut skill = strip_frontmatter(SKILL_CORE).to_string();
    if let Some(reference) = backend_reference(backend) {
        skill.push_str("\n\n---\n\n");
        skill.push_str(reference);
    }
    format!("---\n\n{skill}\n---\n\n")
}

/// Record of a command execution
pub struct CommandExecution {
    pub args: Vec<String>,
    pub output: String,
    /// `None` when the runner cannot tell whether the command failed
    pub is_error: Option<bool>,
}

/// Strip YAML frontmatter (--- ... ---) from skill file content
fn strip_frontmatter(content: &str) -> &str {
    if let Some(stripped) = content.strip_prefix("---\n")
        && let Some(close) = stripped.find("\n---\n")
    {
        return stripped[close + 5..].trim_start();
    }
    content
}

/// Runs an agent session against a scenario
pub struct SessionRunner {
    scenario_path: PathBuf,
    scenario: Scenario,
    run: RunSetup,
    client: Client,
    max_turns: usize,
    verbose: bool,
    messages: Vec<Message>,
}

impl SessionRunner {
    pub fn new(
        scenario_path: PathBuf,
        scenario: Scenario,
        run: RunSetup,
        api_key: String,
        model: String,
        max_turns: usize,
        verbose: bool,
    ) -> Self {
        Self {
            scenario_path,
            scenario,
            run,
            client: Client::new(api_key, model),
            max_turns,
            verbose,
            messages: Vec::new(),
        }
    }

    /// Run the session and return results
    pub fn run(&mut self) -> Result<SessionResult> {
        let mut total_input_tokens = 0;
        let mut total_output_tokens = 0;
        let mut commands_executed = Vec::new();
        let mut turns = 0;

        // Build system prompt
        let system = self.build_system_prompt();

        // Initial user message with the task
        let task_message = self.build_task_message();
        let prompt_bytes = system.len() + task_message.len();
        self.messages.push(Message {
            role: Role::User,
            content: MessageContent::text(task_message),
        });

        if self.verbose {
            println!("\n{}", "Starting agent session...".cyan());
            println!("{}: {}", "Model".dimmed(), self.client.model());
            println!("{}: {}", "Max turns".dimmed(), self.max_turns);
            println!();
        }

        // Agentic loop
        loop {
            if turns == self.max_turns {
                if self.verbose {
                    println!(
                        "{} Maximum turns ({}) reached",
                        "Warning:".yellow(),
                        self.max_turns
                    );
                }
                break;
            }
            turns += 1;

            if self.verbose {
                println!("{} {}", "Turn".cyan().bold(), turns);
            }

            // Send request
            let request = MessageRequest {
                model: self.client.model().to_string(),
                max_tokens: 4096,
                system: Some(system.clone()),
                messages: self.messages.clone(),
                tools: Some(vec![tools::track_cli_tool()]),
            };

            let response = self.client.send_message(&request)?;

            total_input_tokens += response.usage.input_tokens;
            total_output_tokens += response.usage.output_tokens;

            // Process response
            let mut tool_results: Vec<ContentBlock> = Vec::new();

            for block in &response.content {
                match block {
                    ContentBlock::Text { text } => {
                        if self.verbose {
                            println!("{}: {}", "Agent".green(), text);
                        }
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        if name == "track" {
                            match tools::parse_track_input(input) {
                                Ok(args) => {
                                    if self.verbose {
                                        println!(
                                            "{}: track {}",
                                            "Executing".yellow(),
                                            args.join(" ")
                                        );
                                    }

                                    let (output, is_error) = tools::execute_track(
                                        &args,
                                        &self.scenario_path,
                                        &self.run.track_env,
                                    );

                                    if self.verbose {
                                        if is_error {
                                            println!("{}: {}", "Error".red(), output.trim());
                                        } else {
                                            // Truncate long output in verbose mode.
                                            let display_output =
                                                crate::text::truncate_for_display(&output, 500);
                                            println!(
                                                "{}: {}",
                                                "Output".dimmed(),
                                                display_output.trim()
                                            );
                                        }
                                    }

                                    commands_executed.push(CommandExecution {
                                        args: args.clone(),
                                        output: output.clone(),
                                        is_error: Some(is_error),
                                    });

                                    tool_results.push(ContentBlock::tool_result(
                                        id.clone(),
                                        output,
                                        is_error,
                                    ));
                                }
                                Err(e) => {
                                    if self.verbose {
                                        println!("{}: Invalid tool input: {}", "Error".red(), e);
                                    }
                                    tool_results.push(ContentBlock::tool_result(
                                        id.clone(),
                                        format!("Invalid input: {}", e),
                                        true,
                                    ));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Add assistant message
            self.messages.push(Message {
                role: Role::Assistant,
                content: MessageContent::Blocks(response.content.clone()),
            });

            // Check stop reason
            if response.stop_reason == Some(StopReason::EndTurn) {
                if self.verbose {
                    println!("{}", "Agent finished (end_turn)".green());
                }
                break;
            }

            // If there were tool uses, send results
            if !tool_results.is_empty() {
                self.messages.push(Message {
                    role: Role::User,
                    content: MessageContent::Blocks(tool_results),
                });
            } else {
                // No tool use and not end_turn - might be stuck
                if self.verbose {
                    println!(
                        "{} No tool use in response, stop_reason: {:?}",
                        "Warning:".yellow(),
                        response.stop_reason
                    );
                }
                break;
            }

            if self.verbose {
                println!();
            }
        }

        if self.verbose {
            println!(
                "\n{}: {} input, {} output",
                "Tokens used".dimmed(),
                total_input_tokens,
                total_output_tokens
            );
        }

        Ok(SessionResult {
            turns_used: turns,
            total_input_tokens: Some(total_input_tokens),
            total_output_tokens: Some(total_output_tokens),
            commands_executed,
            prompt_bytes,
        })
    }

    /// Build the system prompt using the track skill file
    fn build_system_prompt(&self) -> String {
        let mut prompt = String::new();

        prompt.push_str("# Evaluation Mode\n\n");
        prompt.push_str("You are an AI agent being evaluated on your ability to use the `track` CLI tool efficiently and correctly.\n\n");

        prompt.push_str("## Guidelines\n\n");
        prompt.push_str("1. Use the `track` tool to execute CLI commands\n");
        prompt.push_str("2. Be efficient - minimize the number of commands you use\n");
        prompt.push_str("3. Parse command output to inform your next actions\n");
        prompt.push_str("4. When you've completed the task, simply respond with a summary (no more tool calls)\n");
        prompt.push_str("5. Use -o json for output you need to parse programmatically\n\n");

        prompt.push_str(&self.run.reference);

        if let Some(context) = &self.scenario.setup.context {
            prompt.push_str("## Context\n\n");
            prompt.push_str(context);
            prompt.push_str("\n\n");
        }

        prompt.push_str("Remember: You are being evaluated on both correctness AND efficiency. Complete the task with as few commands as possible while ensuring all requirements are met.");

        prompt
    }

    /// Build the initial task message
    fn build_task_message(&self) -> String {
        format!(
            "## Your Task\n\n{}\n\nPlease complete this task using the track CLI tool.",
            self.scenario.setup.prompt
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario(backend: &str, default_project: &str) -> Scenario {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scenario.toml");
        std::fs::write(
            &path,
            format!(
                r#"
[scenario]
name = "t"
description = "t"
backend = "{backend}"

[setup]
prompt = "Create an issue."
default_project = "{default_project}"
"#
            ),
        )
        .unwrap();
        Scenario::load(&path).expect("scenario parses")
    }

    #[test]
    fn track_env_isolates_the_cache_and_simulates_the_hook_only_in_prime_mode() {
        let cache = Path::new("/tmp/run-cache");
        assert_eq!(
            track_env(ContextMode::Skill, cache, Some("DEMO")),
            vec![("TRACK_CACHE_DIR", "/tmp/run-cache".to_string())]
        );
        assert_eq!(
            track_env(ContextMode::Prime, cache, Some("DEMO")),
            vec![
                ("TRACK_CACHE_DIR", "/tmp/run-cache".to_string()),
                ("TRACKER_DEFAULT_PROJECT", "DEMO".to_string()),
                ("TRACK_OUTPUT", "agent".to_string()),
            ]
        );
        assert_eq!(
            track_env(ContextMode::Prime, cache, None),
            vec![
                ("TRACK_CACHE_DIR", "/tmp/run-cache".to_string()),
                ("TRACK_OUTPUT", "agent".to_string()),
            ]
        );
    }

    #[test]
    fn skill_reference_is_the_shipped_core_plus_the_backend_reference() {
        let core = strip_frontmatter(SKILL_CORE);
        assert!(SKILL_CORE.starts_with("---\n"), "the skill has frontmatter");
        assert!(!core.starts_with("---"), "frontmatter was kept");
        assert!(SKILL_CORE.ends_with(core));

        let jira = skill_reference("Jira");
        assert!(jira.contains(core));
        assert!(jira.contains(backend_reference("jira").unwrap()));
        assert!(!jira.contains(backend_reference("youtrack").unwrap()));

        // A scenario for any backend gets the core only.
        assert_eq!(skill_reference("any"), format!("---\n\n{core}\n---\n\n"));
    }

    #[test]
    fn every_fixture_backend_has_a_reference() {
        for backend in ["youtrack", "jira", "github", "gitlab", "linear"] {
            assert!(backend_reference(backend).is_some(), "{backend}");
        }
        assert!(backend_reference("any").is_none());
    }

    #[test]
    fn prime_reference_wraps_the_briefing_and_drops_failures() {
        let briefing = prime_reference("Project DEMO\nStates: Open, Done\n", false);
        assert!(briefing.starts_with("## Project briefing"), "{briefing}");
        assert!(briefing.contains("```\nProject DEMO\nStates: Open, Done\n```"));

        assert_eq!(prime_reference("error: no config", true), "");
        assert_eq!(prime_reference("  \n", false), "");
    }

    #[test]
    fn prepare_builds_the_reference_without_running_track_outside_prime_mode() {
        let dir = tempfile::tempdir().unwrap();
        let scenario = scenario("youtrack", "DEMO");

        let none = RunSetup::prepare(dir.path(), &scenario, ContextMode::None, dir.path());
        assert_eq!(none.reference, "");
        let skill = RunSetup::prepare(dir.path(), &scenario, ContextMode::Skill, dir.path());
        assert_eq!(skill.reference, skill_reference("youtrack"));
        assert_eq!(
            skill.track_env,
            track_env(ContextMode::Skill, dir.path(), Some("DEMO"))
        );

        // Nothing ran against the mock, so there is no call log to clear.
        assert!(!tracker_mock::call_log_path(dir.path()).exists());
    }

    #[test]
    fn context_usage_counts_the_prompt_and_all_track_output() {
        let session = SessionResult {
            turns_used: 2,
            total_input_tokens: None,
            total_output_tokens: None,
            commands_executed: vec![
                CommandExecution {
                    args: vec!["prime".into()],
                    output: "12345".into(),
                    is_error: Some(false),
                },
                CommandExecution {
                    args: vec!["issue".into()],
                    output: "678".into(),
                    is_error: None,
                },
            ],
            prompt_bytes: 100,
        };
        assert_eq!(
            session.context_usage(),
            ContextUsage {
                prompt_bytes: 100,
                tool_output_bytes: 8,
            }
        );
    }
}
