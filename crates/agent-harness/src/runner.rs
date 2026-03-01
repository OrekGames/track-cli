//! Session runner - manages the conversation with the LLM
//!
//! Handles the agentic loop: prompt -> response -> tool use -> result -> repeat

use crate::anthropic::{
    Client, ContentBlock, Message, MessageContent, MessageRequest, Role, StopReason,
};
use crate::tools;
use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use tracker_mock::Scenario;

/// Result of running a session
#[allow(dead_code)]
pub struct SessionResult {
    pub turns_used: usize,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub commands_executed: Vec<CommandExecution>,
}

/// Record of a command execution
#[allow(dead_code)]
pub struct CommandExecution {
    pub args: Vec<String>,
    pub output: String,
    pub is_error: bool,
}

/// Path to the skill file relative to the project root
const SKILL_FILE_PATH: &str = "agent-skills/SKILL.md";

/// Strip YAML frontmatter (--- ... ---) from skill file content
fn strip_frontmatter(content: &str) -> &str {
    if let Some(stripped) = content.strip_prefix("---\n") {
        if let Some(close) = stripped.find("\n---\n") {
            return stripped[close + 5..].trim_start();
        }
    }
    content
}

/// Load the skill file content, stripping frontmatter
fn load_skill_file() -> Option<String> {
    let paths_to_try = [
        SKILL_FILE_PATH.to_string(),
        format!("./{}", SKILL_FILE_PATH),
        format!("../{}", SKILL_FILE_PATH),
        format!("../../{}", SKILL_FILE_PATH),
    ];

    for path in &paths_to_try {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Some(strip_frontmatter(&content).to_string());
        }
    }

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = std::path::Path::new(&manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // project root
            .map(|p| p.join(SKILL_FILE_PATH));

        if let Some(path) = path {
            if let Ok(content) = std::fs::read_to_string(&path) {
                return Some(strip_frontmatter(&content).to_string());
            }
        }
    }

    None
}

/// Runs an agent session against a scenario
pub struct SessionRunner {
    scenario_path: PathBuf,
    scenario: Scenario,
    client: Client,
    max_turns: usize,
    verbose: bool,
    messages: Vec<Message>,
}

impl SessionRunner {
    pub fn new(
        scenario_path: PathBuf,
        scenario: Scenario,
        api_key: String,
        model: String,
        max_turns: usize,
        verbose: bool,
    ) -> Self {
        Self {
            scenario_path,
            scenario,
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
            turns += 1;
            if turns > self.max_turns {
                if self.verbose {
                    println!(
                        "{} Maximum turns ({}) reached",
                        "Warning:".yellow(),
                        self.max_turns
                    );
                }
                break;
            }

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

                                    let (output, is_error) =
                                        tools::execute_track(&args, &self.scenario_path);

                                    if self.verbose {
                                        if is_error {
                                            println!("{}: {}", "Error".red(), output.trim());
                                        } else {
                                            // Truncate long output in verbose mode
                                            let display_output = if output.len() > 500 {
                                                format!("{}...", &output[..500])
                                            } else {
                                                output.clone()
                                            };
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
                                        is_error,
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
            total_input_tokens,
            total_output_tokens,
            commands_executed,
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

        // Load and include the skill file (same context a real user would have installed)
        if let Some(skill_content) = load_skill_file() {
            prompt.push_str("---\n\n");
            prompt.push_str(&skill_content);
            prompt.push_str("\n---\n\n");
        } else {
            // Fallback to minimal reference if skill file not found
            prompt.push_str("## Track CLI Quick Reference\n\n");
            prompt.push_str("Note: Skill file not found. Using minimal reference.\n\n");
            prompt.push_str("```\n");
            prompt.push_str("track issue get <ID>              # Get issue details\n");
            prompt.push_str("track issue search <query>        # Search issues\n");
            prompt.push_str("track issue create -p <proj> -s <summary>\n");
            prompt.push_str("track issue update <ID> [--state <state>] [--summary <summary>]\n");
            prompt.push_str("track issue comment <ID> -m <message>\n");
            prompt.push_str("track issue comments <ID>         # List comments\n");
            prompt.push_str("track project list                # List projects\n");
            prompt.push_str("track project fields <ID>         # List custom fields\n");
            prompt.push_str("```\n\n");
        }

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
