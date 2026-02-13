# Agent Harness

Evaluate AI agents on their ability to use the `track` CLI efficiently and correctly.

## Overview

The agent harness:
1. Loads a scenario (task prompt + expected outcomes + mock responses)
2. Presents the task to an LLM via the Anthropic API or Claude Code CLI
3. Provides a `track` tool that executes CLI commands in mock mode
4. Logs all commands the agent executes
5. Evaluates performance based on correctness and efficiency

## Installation

```bash
# Build the harness
cargo build --release --package agent-harness

# Binary is at ./target/release/track-agent
```

## Providers

The harness supports three providers for running agents:

### Anthropic API (default)

Uses the Anthropic Messages API directly with a custom agentic loop.

```bash
export ANTHROPIC_API_KEY=your-key-here
track-agent run ./fixtures/scenarios/basic-workflow --provider anthropic
```

### Claude Code CLI

Uses the Claude Code CLI (`claude`) as a subprocess. This evaluates how Claude Code specifically performs on scenarios, including its tool orchestration and permission handling.

```bash
# Requires Claude Code CLI to be installed
track-agent run ./fixtures/scenarios/basic-workflow --provider claude-code -v
```

Key differences:
- No API key needed (Claude Code handles authentication)
- Uses Claude Code's system prompt and behaviors
- Tool restrictions via `--allowedTools` flag
- Token counts not available (only turn counts)

### GitHub Copilot CLI

Uses the GitHub Copilot CLI (`gh copilot`) as a subprocess. This evaluates how GitHub Copilot CLI performs on scenarios using its bash tool integration.

```bash
# Requires GitHub CLI (gh) and Copilot CLI extension to be installed
# gh extension install github/gh-copilot
track-agent run ./fixtures/scenarios/basic-workflow --provider copilot-cli -v
```

Key differences:
- No API key needed (GitHub CLI handles authentication via `gh auth login`)
- Uses GitHub Copilot's system prompt and behaviors
- Tool restrictions via `--available-tools` flag
- Token counts not available (only turn counts)
- Commands tracked via mock backend's call log

## Usage

### Running a Single Scenario

```bash
# Run with verbose output to see agent thinking
track-agent run ./fixtures/scenarios/basic-workflow -v

# Use Claude Code CLI instead of direct API
track-agent run ./fixtures/scenarios/basic-workflow --provider claude-code -v

# Use GitHub Copilot CLI
track-agent run ./fixtures/scenarios/basic-workflow --provider copilot-cli -v

# Use a specific model (Anthropic provider only)
track-agent run ./fixtures/scenarios/basic-workflow --model claude-sonnet-4-20250514

# Set minimum passing score
track-agent run ./fixtures/scenarios/basic-workflow --min-score 80

# JSON output for CI
track-agent run ./fixtures/scenarios/basic-workflow -o json
```

### Running All Scenarios

```bash
# Run all scenarios in a directory
track-agent run-all --path ./fixtures/scenarios

# Run all scenarios with Claude Code
track-agent run-all --path ./fixtures/scenarios --provider claude-code

# Run all scenarios with GitHub Copilot CLI
track-agent run-all --path ./fixtures/scenarios --provider copilot-cli

# Stop on first failure
track-agent run-all --path ./fixtures/scenarios --fail-fast

# Set minimum score threshold
track-agent run-all --path ./fixtures/scenarios --min-score 70
```

### Listing Scenarios

```bash
track-agent list --path ./fixtures/scenarios
```

### Viewing Scenario Details

```bash
track-agent show ./fixtures/scenarios/basic-workflow
```

## How It Works

### Anthropic Provider

1. **System Prompt**: The agent receives guidelines about using `track` efficiently
2. **Task Prompt**: The scenario's task description is presented
3. **Tool Use**: The agent can call `track` with any arguments
4. **Evaluation**: Commands are executed in mock mode and logged
5. **Iteration**: The agent receives results and can make more tool calls
6. **Completion**: The agent signals completion or hits max turns

The agent has access to one tool - `track` - which executes CLI commands:

```json
{
  "name": "track",
  "description": "Execute a track CLI command...",
  "input_schema": {
    "type": "object",
    "properties": {
      "args": {
        "type": "array",
        "items": { "type": "string" },
        "description": "Command line arguments"
      }
    }
  }
}
```

### Claude Code Provider

1. **System Prompt**: Written to a temp file, includes evaluation guidelines
2. **Task Prompt**: Piped to Claude Code's stdin
3. **Tool Restriction**: Claude Code limited to `Bash(track *)` only
4. **Mock Mode**: `TRACK_MOCK_DIR` environment variable routes all track commands through the mock system
5. **Event Parsing**: Stream JSON output parsed for tool use and results
6. **Completion**: Claude Code finishes naturally or hits max turns

Claude Code invocation:
```bash
claude -p \
  --output-format stream-json \
  --allowedTools "Bash(/path/to/track *)" \
  --system-prompt ./scenario-system-prompt.md \
  --dangerously-skip-permissions \
  --max-turns 20
```

### GitHub Copilot CLI Provider

1. **System Prompt**: Written to `AGENTS.md` in temp directory, automatically loaded by Copilot CLI
2. **Task Prompt**: Passed via `-p` flag in non-interactive mode
3. **Tool Restriction**: Copilot CLI limited to `bash` tool only via `--available-tools`
4. **Mock Mode**: `TRACK_MOCK_DIR` environment variable routes all track commands through the mock system
5. **Output Parsing**: Stdout and stderr captured for analysis
6. **Command Tracking**: Commands extracted from mock backend's call log
7. **Completion**: Copilot CLI finishes naturally (no explicit max turns control)

Copilot CLI invocation:
```bash
gh copilot -- -p "task prompt" \
  --allow-all-tools \
  --allow-all-paths \
  --silent \
  --available-tools bash
```

### Evaluation Metrics

After the session, the harness evaluates:

| Metric | Description |
|--------|-------------|
| **Score** | Points based on outcomes achieved and efficiency |
| **Outcomes** | Whether expected actions were performed |
| **Efficiency** | Commands used vs optimal |
| **Turns** | Number of LLM round trips |

## Example Session

### Anthropic Provider

```
$ track-agent run ./fixtures/scenarios/basic-workflow -v

Starting agent session...
Model: claude-sonnet-4-20250514
Max turns: 20

Turn 1
Agent: I'll help you complete this task. Let me start by getting the current state of issue DEMO-1.
Executing: track issue get DEMO-1
Output: DEMO-1 - Implement user authentication
  State: Open
  ...

Turn 2
Agent: The issue is currently Open. Now I'll add a comment and update the state.
Executing: track issue comment DEMO-1 -m "Starting work on this issue"
Output: Comment added to DEMO-1

Turn 3
Executing: track issue update DEMO-1 --state "In Progress"
Output: DEMO-1 - Implement user authentication
  State: In Progress
  ...

Agent: I've completed all three tasks efficiently in 3 commands.
Agent finished (end_turn)

════════════════════════════════════════════════════════════
EVALUATION RESULTS
════════════════════════════════════════════════════════════

Scenario: basic-workflow - PASS
Score: 105/100 (100%)
Commands: 3 (optimal: 4)
Turns used: 3
Efficiency: Excellent

Expected Outcomes:
  ✓ issue_fetched
  ✓ comment_added
  ✓ state_updated
```

### Claude Code Provider

```
$ track-agent run ./fixtures/scenarios/basic-workflow --provider claude-code -v

Starting Claude Code session...
Scenario: basic-workflow
Max turns: 20
Track binary: /path/to/target/debug/track

Agent: I'll get the issue details first.
Executing: track issue get DEMO-1
Output: DEMO-1 - Implement user authentication...

Agent: Now adding a comment.
Executing: track issue comment DEMO-1 -m "Starting work on this issue"
Output: Comment added to DEMO-1

Agent: Finally, updating the state.
Executing: track issue update DEMO-1 --state "In Progress"
Output: DEMO-1 updated...

Final result: Completed all tasks: fetched issue, added comment, updated state.

Session complete: 3 commands in 4 turns

════════════════════════════════════════════════════════════
EVALUATION RESULTS
════════════════════════════════════════════════════════════

Scenario: basic-workflow - PASS
Score: 105/100 (100%)
Commands: 3 (optimal: 4)
Turns used: 4
Efficiency: Excellent
```

## CI Integration

Use exit codes to integrate with CI:

### Anthropic Provider

```yaml
- name: Run agent evaluation
  run: |
    ./target/release/track-agent run ./fixtures/scenarios/basic-workflow \
      --min-score 80 \
      -o json
  env:
    ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

### Claude Code Provider

```yaml
- name: Run agent evaluation with Claude Code
  run: |
    ./target/release/track-agent run ./fixtures/scenarios/basic-workflow \
      --provider claude-code \
      --min-score 80 \
      -o json
```

Note: Claude Code must be installed and authenticated on the CI runner.

### GitHub Copilot CLI Provider

```yaml
- name: Run agent evaluation with GitHub Copilot CLI
  run: |
    # Authenticate with GitHub CLI first
    echo "${{ secrets.GITHUB_TOKEN }}" | gh auth login --with-token
    
    # Run evaluation
    ./target/release/track-agent run ./fixtures/scenarios/basic-workflow \
      --provider copilot-cli \
      --min-score 80 \
      -o json
```

Note: GitHub CLI and Copilot extension must be installed on the CI runner.

Exit codes:
- `0`: All evaluations passed
- `1`: One or more evaluations failed

## Creating New Scenarios

See `fixtures/README.md` for details on creating scenarios with:
- Task prompts for agents
- Expected outcomes
- Mock API responses
- Scoring configuration
