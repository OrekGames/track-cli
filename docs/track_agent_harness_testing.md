# Agent Evaluation Harness

This document describes the agent evaluation harness (`track-agent`) which evaluates AI agents on their ability to use the `track` CLI efficiently and correctly.

## Overview

The harness supports two providers for running agent evaluations:

1. **Anthropic API** (default) - Direct API calls with custom agentic loop
2. **Claude Code CLI** - Uses the `claude` CLI as a subprocess

Both providers execute scenarios against a mock system, log all commands, and evaluate performance based on correctness and efficiency.

## Architecture

### Dual Provider Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           track-agent                                    │
│                        (Rust binary)                                     │
└─────────────────────────────┬───────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              │                               │
              ▼                               ▼
┌─────────────────────────┐     ┌─────────────────────────┐
│   Anthropic Provider    │     │  Claude Code Provider   │
│   (runner.rs)           │     │  (claude_code.rs)       │
│                         │     │                         │
│  - Direct API calls     │     │  - Subprocess spawn     │
│  - Custom tool def      │     │  - Stream JSON parsing  │
│  - Token tracking       │     │  - Bash tool restrict   │
└───────────┬─────────────┘     └───────────┬─────────────┘
            │                               │
            │    ┌─────────────────────┐    │
            └───▶│   track CLI         │◀───┘
                 │   (mock mode)       │
                 └──────────┬──────────┘
                            │
                            ▼
                 ┌─────────────────────┐
                 │   MockClient        │
                 │   (fixture data)    │
                 │                     │
                 │  - Response files   │
                 │  - Call logging     │
                 └─────────────────────┘
                            │
                            ▼
                 ┌─────────────────────┐
                 │   Evaluator         │
                 │                     │
                 │  - Outcome checks   │
                 │  - Scoring          │
                 │  - Efficiency       │
                 └─────────────────────┘
```

## Provider Details

### Anthropic API Provider

The default provider uses the Anthropic Messages API directly.

**How it works:**

1. Builds a system prompt with evaluation guidelines and available commands
2. Sends the scenario task as the initial user message
3. Defines a `track` tool that the model can call with CLI arguments
4. Executes tool calls against the mock system
5. Returns tool results to continue the conversation
6. Loops until the model signals completion or max turns reached

**Advantages:**
- Full token usage tracking (input/output)
- Direct control over tool definitions
- Model selection via `--model` flag

**Implementation:** `crates/agent-harness/src/runner.rs`

```rust
// Tool definition for Anthropic API
Tool {
    name: "track".to_string(),
    description: "Execute a track CLI command...",
    input_schema: json!({
        "type": "object",
        "properties": {
            "args": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Command line arguments"
            }
        },
        "required": ["args"]
    }),
}
```

### Claude Code CLI Provider

Uses the `claude` CLI tool as a subprocess to evaluate Claude Code specifically.

**How it works:**

1. Builds a system prompt appended to Claude Code's default prompt
2. Spawns `claude` with specific flags for automation
3. Restricts tools to only allow `Bash` with the track binary
4. Parses streaming JSON output for events and tool use
5. Extracts track commands from Bash tool invocations
6. Routes all track commands through mock system via `TRACK_MOCK_DIR`

**Advantages:**
- Tests Claude Code's actual behavior and tool orchestration
- Uses Claude Code's built-in system prompt and capabilities
- Real-world evaluation of the production tool

**Implementation:** `crates/agent-harness/src/claude_code.rs`

**Required CLI Flags:**

| Flag | Purpose |
|------|---------|
| `-p, --print` | Non-interactive mode (no TUI) |
| `--output-format stream-json` | Machine-readable streaming output |
| `--verbose` | Required when using stream-json with --print |
| `--allowedTools "Bash(/path/to/track *)"` | Restrict to track CLI only |
| `--append-system-prompt` | Add evaluation guidelines |
| `--dangerously-skip-permissions` | Bypass permission prompts |
| `--max-turns` | Limit agentic turns |

**Actual Invocation:**

```rust
let mut cmd = Command::new("claude");
cmd.args([
    "-p",
    "--output-format", "stream-json",
    "--verbose",  // Required for stream-json with --print
    "--allowedTools", &format!("Bash({} *)", track_bin),
    "--append-system-prompt", &system_prompt,
    "--dangerously-skip-permissions",
    "--max-turns", &config.max_turns.to_string(),
    &task_prompt,  // Prompt as final argument (not stdin)
]);
cmd.env("TRACK_MOCK_DIR", &config.scenario_path);
```

**Stream JSON Event Types:**

```rust
pub enum ClaudeCodeEvent {
    System(SystemEvent),      // Init, session info
    Assistant(AssistantEvent), // Model responses, tool use
    User(UserEvent),          // Tool results
    Result(ResultEvent),      // Final result, duration
}
```

## Mock System

Both providers route track commands through the mock system:

1. **Environment Variable**: `TRACK_MOCK_DIR` points to scenario directory
2. **Manifest**: `manifest.toml` maps requests to response files
3. **Call Logging**: All calls logged to `call_log.jsonl`
4. **Response Files**: JSON responses in `responses/` directory

```
fixtures/scenarios/basic-workflow/
├── scenario.toml      # Task, outcomes, scoring
├── manifest.toml      # Request → response mapping
├── call_log.jsonl     # Runtime log (written by mock)
└── responses/
    ├── get_issue_DEMO-1.json
    ├── list_projects.json
    └── ...
```

## Evaluation

The evaluator analyzes `call_log.jsonl` against scenario expectations:

### Outcome Checking

Outcomes are defined in `scenario.toml`:

```toml
[expected_outcomes]
# Simple: check if issue ID was referenced
issue_fetched = "DEMO-1"

# Complex: check method, issue, and content
comment_added = { method_called = "add_comment", issue = "DEMO-1", contains = "Starting" }

# For create operations: checks summary field
issue_created = { method_called = "create_issue", contains = "login" }
```

The `contains` field checks different locations based on `method_called`:
- `create_issue` → checks `summary` argument
- `add_comment` → checks `text` argument
- Other methods → checks all string arguments

### Scoring

```toml
[scoring]
min_commands = 3        # Theoretical minimum
max_commands = 6        # Acceptable maximum
optimal_commands = 4    # Expected for good agent
base_score = 100

[scoring.penalties]
extra_command = -5      # Per command over max
redundant_fetch = -10   # Same resource fetched twice
command_error = -15     # Per failed command

[scoring.bonuses]
cache_use = 10          # If cache commands used
under_optimal = 5       # Per command under optimal
```

### Efficiency Ratings

| Rating | Condition |
|--------|-----------|
| Excellent | Commands < optimal |
| Optimal | Commands = optimal |
| Acceptable | optimal < commands ≤ max |
| Inefficient | Commands > max |

## Usage

### Build

```bash
cargo build --package agent-harness
# Binary: ./target/debug/track-agent
```

### Run Single Scenario

```bash
# With Anthropic API (default)
export ANTHROPIC_API_KEY=your-key
./target/debug/track-agent run ./fixtures/scenarios/basic-workflow -v

# With Claude Code CLI
./target/debug/track-agent run ./fixtures/scenarios/basic-workflow --provider claude-code -v
```

### Run All Scenarios

```bash
# Anthropic API
./target/debug/track-agent run-all --path ./fixtures/scenarios

# Claude Code CLI
./target/debug/track-agent run-all --path ./fixtures/scenarios --provider claude-code
```

### Options

```bash
track-agent run <SCENARIO> [OPTIONS]

Options:
  --provider <PROVIDER>    anthropic (default) or claude-code
  --model <MODEL>          Model for Anthropic provider
  --max-turns <N>          Maximum agentic turns (default: 20)
  --min-score <N>          Minimum passing score (default: 70)
  -v, --verbose            Show detailed output
  -o, --format <FORMAT>    text (default) or json
```

### Example Output

```
$ ./target/debug/track-agent run ./fixtures/scenarios/basic-workflow --provider claude-code -v

Starting Claude Code session...
Scenario: basic-workflow
Max turns: 20
Track binary: /path/to/target/debug/track

Executing: track issue get DEMO-1 -o json
Output: { "id": "2-1", "id_readable": "DEMO-1", ... }

Agent: Issue DEMO-1 is in "Open" state. Adding comment and updating state.

Executing: track issue comment DEMO-1 -m "Starting work on this issue"
Output: Comment added to DEMO-1

Executing: track issue update DEMO-1 --state "In Progress"
Output: DEMO-1 - Implement user authentication...

Agent: Task completed successfully.

Session complete: 3 commands in 5 turns

════════════════════════════════════════════════════════════
EVALUATION RESULTS
════════════════════════════════════════════════════════════

Scenario: basic-workflow - PASS
Score: 105/100 (100%)
Commands: 3 (optimal: 4)
Turns used: 5
Efficiency: Excellent

Expected Outcomes:
  ✓ issue_fetched
  ✓ comment_added
  ✓ state_updated
```

## Test Results

Current scenario results with Claude Code provider:

```
Running: error-recovery...
  PASS - 95% (4 calls, 7 turns)

Running: basic-workflow...
  PASS - 100% (3 calls, 3 turns)

Running: cache-efficiency...
  PASS - 100% (6 calls, 10 turns)

────────────────────────────────────────────────────────────
  ✓ 3/3 scenarios passed
```

## CI Integration

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
    ./target/release/track-agent run-all \
      --path ./fixtures/scenarios \
      --provider claude-code \
      --min-score 80 \
      -o json
```

Note: Claude Code must be installed and authenticated on the CI runner.

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All evaluations passed |
| 1 | One or more evaluations failed |

## Implementation Notes

### Key Files

| File | Purpose |
|------|---------|
| `crates/agent-harness/src/main.rs` | CLI entry point, provider dispatch |
| `crates/agent-harness/src/runner.rs` | Anthropic API session runner |
| `crates/agent-harness/src/claude_code.rs` | Claude Code CLI runner |
| `crates/agent-harness/src/anthropic.rs` | Anthropic API client |
| `crates/agent-harness/src/tools.rs` | Tool definitions and execution |
| `crates/tracker-mock/src/evaluator.rs` | Outcome checking and scoring |

### Dependencies

```toml
[dependencies]
tracker-mock = { path = "../tracker-mock" }
clap = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
colored = { workspace = true }
ureq = { workspace = true }
chrono = { workspace = true }
shell-words = "1.1"  # For parsing quoted arguments
```

### Provider Comparison

| Feature | Anthropic API | Claude Code CLI |
|---------|---------------|-----------------|
| Token tracking | Yes | No |
| Model selection | Yes | No (uses default) |
| Tool definition | Custom `track` tool | Bash with path restriction |
| System prompt | Full control | Appended to default |
| API key required | Yes | No (CLI handles auth) |
| Subprocess | No | Yes |

## Future Enhancements

1. **Comparative Evaluation**: Run same scenario with both providers, compare results
2. **Cost Tracking**: Parse Claude Code logs for token usage if available
3. **Parallel Execution**: Run multiple scenarios in parallel
4. **Custom Model Selection**: Pass `--model` flag through to Claude Code
5. **Timeout Handling**: Add overall timeout for runaway sessions
6. **Result Caching**: Cache evaluation results for regression testing
