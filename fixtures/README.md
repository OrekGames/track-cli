# Mock System for AI Agent Evaluation

This directory contains the mock system for evaluating AI agents using the `track` CLI.

## Overview

The mock system allows you to:
1. **Create reproducible scenarios** with predefined API responses
2. **Log all CLI calls** for analysis
3. **Evaluate agent performance** based on correctness and efficiency
4. **Run tests** without a real YouTrack/Jira instance

## Quick Start

### Running a Scenario

Set `TRACK_MOCK_DIR` to point to a scenario directory:

```bash
# Run track commands against mock data
TRACK_MOCK_DIR=./fixtures/scenarios/basic-workflow track issue get DEMO-1

# All API calls are logged to call_log.jsonl
cat ./fixtures/scenarios/basic-workflow/call_log.jsonl
```

### Example: Evaluating an AI Agent

1. **Clear the call log** before starting:
   ```bash
   > ./fixtures/scenarios/basic-workflow/call_log.jsonl
   ```

2. **Set the environment and run the agent**:
   ```bash
   export TRACK_MOCK_DIR=./fixtures/scenarios/basic-workflow
   # ... AI agent executes track commands ...
   ```

3. **Analyze the results** using the evaluator (see Programmatic Usage below)

## Directory Structure

```
fixtures/
├── README.md              # This file
└── scenarios/
    └── basic-workflow/    # A single test scenario
        ├── scenario.toml  # Metadata & evaluation criteria
        ├── manifest.toml  # Request → response mapping
        ├── call_log.jsonl # Runtime log (written by MockClient)
        └── responses/     # JSON response files
            ├── get_issue_DEMO-1.json
            ├── list_projects.json
            └── ...
```

## Creating a New Scenario

### 1. Create the directory structure

```bash
mkdir -p fixtures/scenarios/my-scenario/responses
```

### 2. Create `scenario.toml`

```toml
[scenario]
name = "my-scenario"
description = "Description of what the agent should accomplish"
backend = "youtrack"  # or "jira" or "any"
difficulty = "easy"   # easy, medium, hard
tags = ["issues", "comments"]

[setup]
prompt = """
Task prompt for the AI agent.
Describe what they should accomplish.
"""
default_project = "DEMO"
cache_available = true

[expected_outcomes]
# Simple string match (any call arg should contain this)
issue_fetched = "DEMO-1"

# Complex outcome with multiple checks
comment_added = { method_called = "add_comment", issue = "DEMO-1", contains = "text" }
state_updated = { method_called = "update_issue", issue = "DEMO-1" }

[scoring]
min_commands = 3        # Theoretical minimum
max_commands = 6        # Acceptable maximum
optimal_commands = 4    # Expected for a good agent
base_score = 100

[scoring.penalties]
extra_command = -5      # Per command over max
redundant_fetch = -10   # Fetching same resource twice
command_error = -15     # Per failed command

[scoring.bonuses]
cache_use = 10          # If cache is used effectively
under_optimal = 5       # Per command under optimal
```

### 3. Create `manifest.toml`

```toml
# Map method calls to response files

[[responses]]
method = "get_issue"
file = "get_issue_DEMO-1.json"
[responses.args]
id = "DEMO-1"

[[responses]]
method = "list_projects"
file = "list_projects.json"

# Wildcard matching
[[responses]]
method = "search_issues"
file = "search_results.json"
[responses.args]
query = "*"  # Matches any query

# Error response
[[responses]]
method = "get_issue"
file = "error_404.json"
status = 404
[responses.args]
id = "NOTFOUND-1"

# Sequence responses (different response each call)
[[responses]]
method = "get_issue"
sequence = ["issue_open.json", "issue_done.json"]
[responses.args]
id = "DEMO-2"
```

### 4. Create response files

Each response file should contain valid JSON matching the `tracker-core` models.
See `scenarios/basic-workflow/responses/` for examples.

## Programmatic Usage

### Using MockClient in Tests

```rust
use tracker_mock::MockClient;
use tracker_core::IssueTracker;

let client = MockClient::new("./fixtures/scenarios/basic-workflow")?;

// Use like a normal IssueTracker
let issue = client.get_issue("DEMO-1")?;
println!("Issue: {}", issue.summary);

// Check call count
println!("Total calls: {}", client.call_count());

// Read call log for analysis
let calls = client.read_call_log()?;
```

### Evaluating Results

```rust
use tracker_mock::{Evaluator, Scenario};

// Load scenario
let scenario = Scenario::load_from_dir("./fixtures/scenarios/basic-workflow")?;
let evaluator = Evaluator::new(scenario);

// Load call log
let client = MockClient::new("./fixtures/scenarios/basic-workflow")?;
let calls = client.read_call_log()?;

// Evaluate
let result = evaluator.evaluate(&calls);

println!("Success: {}", result.success);
println!("Score: {}/{}", result.score, result.max_score);
println!("Efficiency: {:?}", result.efficiency);

for outcome in &result.outcomes {
    println!("  {}: {} (expected: {}, actual: {})",
        outcome.name,
        if outcome.achieved { "✓" } else { "✗" },
        outcome.expected,
        outcome.actual
    );
}

for suggestion in &result.suggestions {
    println!("  Tip: {}", suggestion);
}
```

## Evaluation Metrics

### Correctness Metrics

| Metric | Description |
|--------|-------------|
| `success` | All expected outcomes achieved |
| `outcomes` | Individual outcome pass/fail status |

### Efficiency Metrics

| Metric | Description |
|--------|-------------|
| `total_calls` | Number of CLI commands executed |
| `optimal_calls` | Expected number for a good agent |
| `efficiency` | Rating: Excellent, Optimal, Acceptable, Inefficient |
| `redundant_fetch` | Same resource fetched multiple times |

### Score Breakdown

The score starts at `base_score` (default: 100) and is adjusted by:

**Penalties:**
- `-25` per failed expected outcome
- `extra_command` per command over `max_commands`
- `redundant_fetch` per duplicate fetch
- `command_error` per failed command

**Bonuses:**
- `cache_use` if cache was utilized
- `under_optimal` per command under `optimal_commands`

## Included Scenarios

### basic-workflow (Easy)
Tests basic issue operations: fetch, comment, update state.
- **Goal**: Get DEMO-1, add a comment, mark as In Progress
- **Optimal**: 4 commands
- **Tests**: Issue lookup, commenting, state changes

### error-recovery (Medium)
Tests error handling and recovery.
- **Goal**: Handle 404 for missing issue, recover by searching
- **Optimal**: 5 commands
- **Tests**: Error handling, search fallback, resilience

### cache-efficiency (Medium)
Tests efficient use of cached project data.
- **Goal**: Create 3 issues without redundant API calls
- **Optimal**: 4 commands
- **Tests**: Cache usage, batch operations, efficiency

## CI Integration

The evaluation system integrates with CI pipelines via exit codes and batch commands.

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All checks passed |
| 1 | Evaluation failed (score below threshold or outcomes not met) |

### CI-friendly Commands

```bash
# Run with minimum score threshold (exit 1 if below)
track eval run ./fixtures/scenarios/basic-workflow --min-score 80

# Strict mode: require all expected outcomes
track eval run ./fixtures/scenarios/basic-workflow --strict

# Run all scenarios at once
track eval run-all --path ./fixtures/scenarios --min-score 70

# Fail fast on first failure
track eval run-all --path ./fixtures/scenarios --fail-fast

# JSON output for parsing
track eval run ./fixtures/scenarios/basic-workflow -o json
```

### Integration Tests

The project includes integration tests in `crates/track/tests/eval_integration.rs` that:
- Verify mock mode works correctly
- Test all eval commands
- Validate scenario evaluation

Run with: `cargo test --package track --test eval_integration`

## Agent Harness

For automated AI agent evaluation, use the `track-agent` binary:

```bash
# Build the harness
cargo build --release --package agent-harness

# Set API key
export ANTHROPIC_API_KEY=your-key-here

# Run an agent against a scenario
./target/release/track-agent run ./fixtures/scenarios/basic-workflow -v

# Run all scenarios
./target/release/track-agent run-all --path ./fixtures/scenarios
```

The harness:
1. Presents the scenario prompt to Claude via the Anthropic API
2. Provides a `track` tool that executes CLI commands in mock mode
3. Evaluates the agent's performance on correctness and efficiency

See `crates/agent-harness/README.md` for full documentation.

## Best Practices

1. **Keep scenarios focused** - Test one workflow at a time
2. **Use realistic data** - Response files should mirror real API responses
3. **Document expected behavior** - Make the prompt clear about what success looks like
4. **Set reasonable bounds** - `min_commands` and `max_commands` should reflect reality
5. **Test error handling** - Include 404 and other error scenarios
