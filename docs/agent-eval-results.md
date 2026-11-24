# Agent Evaluation Results

This document tracks the performance of different AI providers when using the `track` CLI tool. Tests are run using the `track-agent` evaluation harness against mock scenarios.

## Test Configuration

- **Evaluation Date**: 2026-03-02
- **Max Turns**: 20
- **Min Passing Score**: 70%
- **Total Scenarios**: 21

## Context cost baseline (before the agent-efficiency work)

**Provider**: `claude-code` · **Date**: 2026-11-24 · **track**: 1.19.3 plus the measurement changes · **Context**: full skill file

The harness now records what each run cost in context, not only how many backend
calls it made. `Prompt` is the system prompt plus the task; `Output` is every byte
of `track` output the agent read. Each scenario ran once with its hints and extra
context (`--hints on`) and once without (`--hints off`).

| Scenario | Score (hints) | Calls | Output bytes | Score (no hints) | Calls | Output bytes |
|----------|---------------|-------|--------------|------------------|-------|--------------|
| article-workflow | 90% | 11 | 1207 | 90% | 11 | 1293 |
| basic-workflow | 90% | 5 | 1100 | 65% | 9 | 2258 |
| cache-efficiency | 75% | 12 | 0 | 75% | 12 | 0 |
| cache-operations | 75% | 12 | 1236 | 75% | 12 | 1236 |
| config-management | 100% | 3 | 198 | 100% | 3 | 198 |
| context-aggregation | 90% | 7 | 1273 | 90% | 7 | 1273 |
| error-recovery | 95% | 4 | 803 | 95% | 4 | 506 |
| github-basic-workflow | 100% | 4 | 3099 | 100% | 6 | 1153 |
| issue-batch-operations | 0% | 31 | 6799 | 0% | 30 | 6652 |
| issue-comments-listing | 90% | 5 | 645 | 90% | 5 | 614 |
| issue-create-advanced | 100% | 11 | 826 | 100% | 11 | 3606 |
| issue-delete | 100% | 5 | 255 | 100% | 5 | 436 |
| issue-get-full | 90% | 8 | 1068 | 100% | 6 | 678 |
| issue-history | 90% | 3 | 1780 | 90% | 3 | 1788 |
| issue-linking | 100% | 5 | 2599 | 100% | 3 | 2454 |
| issue-search-templates | 95% | 11 | 9435 | 95% | 11 | 4270 |
| issue-start-complete | 90% | 8 | 130 | 90% | 8 | 130 |
| jira-basic-workflow | 60% | 9 | 2422 | 80% | 7 | 919 |
| json-output | 100% | 4 | 1859 | 100% | 4 | 1859 |
| project-operations | 100% | 6 | 1239 | 100% | 6 | 1239 |
| state-field-resolution | 90% | 6 | 60 | 90% | 6 | 60 |
| tags-operations | 100% | 7 | 697 | 100% | 7 | 882 |
| validation-dry-run | 90% | 9 | 301 | 90% | 9 | 315 |

| Totals over 23 scenarios | Hints on | Hints off |
|---|---|---|
| Passed | 21/23 | 21/23 |
| Average score | 87.4% | 87.6% |
| Backend calls | 186 | 185 |
| Tool output read | 39031 bytes | 33819 bytes |
| Prompt per run | 47948 bytes (~11987 tokens) | 47767 bytes (~11941 tokens) |

What this shows:

- The prompt dominates. About 12,000 tokens of every run are the skill file; all the
  `track` output an agent reads in a scenario averages under 500 tokens.
- Withholding hints barely moves scores or call counts here, because the skill file
  already carries the same advice.
- `gitlab-basic-workflow` uses an older scenario format the harness skips, so 23 of
  the 24 scenario directories ran.
- These are single runs. Differences of a call or two between the columns are noise.

Reproduce with:

```bash
cargo build -p track -p agent-harness
./target/debug/track-agent run-all --provider claude-code --hints on -o json
./target/debug/track-agent run-all --provider claude-code --hints off -o json
```

`--context skill|prime|none` chooses what reference material the agent starts with.

---

## Results by Provider

### Claude Code (CLI)

**Provider**: `claude-code`
**Evaluation Date**: 2026-03-02
**Pass Rate**: 21/21 (100%)

| Scenario | Score | API Calls | Turns | Status |
|----------|-------|-----------|-------|--------|
| article-workflow | 90% | 11 | 11 | ✓ PASS |
| basic-workflow | 100% | 3 | 5 | ✓ PASS |
| cache-efficiency | 90% | 9 | 2 | ✓ PASS |
| cache-operations | 75% | 12 | 2 | ✓ PASS |
| config-management | 100% | 3 | 2 | ✓ PASS |
| context-aggregation | 95% | 6 | 2 | ✓ PASS |
| error-recovery | 95% | 3 | 6 | ✓ PASS |
| github-basic-workflow | 100% | 4 | 6 | ✓ PASS |
| issue-batch-operations | 85% | 15 | 5 | ✓ PASS |
| issue-comments-listing | 90% | 5 | 6 | ✓ PASS |
| issue-create-advanced | 100% | 11 | 11 | ✓ PASS |
| issue-delete | 100% | 5 | 4 | ✓ PASS |
| issue-get-full | 90% | 8 | 8 | ✓ PASS |
| issue-linking | 100% | 4 | 4 | ✓ PASS |
| issue-search-templates | 100% | 6 | 8 | ✓ PASS |
| issue-start-complete | 100% | 4 | 5 | ✓ PASS |
| jira-basic-workflow | 90% | 6 | 5 | ✓ PASS |
| json-output | 100% | 4 | 4 | ✓ PASS |
| project-operations | 100% | 6 | 5 | ✓ PASS |
| tags-operations | 100% | 6 | 6 | ✓ PASS |
| validation-dry-run | 90% | 9 | 3 | ✓ PASS |

**Average Score**: 95%
**Average Turns**: 5.2

---

### Anthropic API

**Provider**: `anthropic`
**Status**: Not yet tested

| Scenario | Score | API Calls | Turns | Status |
|----------|-------|-----------|-------|--------|
| — | — | — | — | — |

---

### OpenAI / ChatGPT

**Provider**: `openai`
**Status**: Not yet tested

| Scenario | Score | API Calls | Turns | Status |
|----------|-------|-----------|-------|--------|
| — | — | — | — | — |

---

### GitHub Copilot CLI

**Provider**: `copilot-cli`
**Evaluation Date**: 2026-02-17
**Pass Rate**: 0/21 (0%)

| Scenario | Score | API Calls | Status |
|----------|-------|-----------|--------|
| article-workflow | 0% | 0 | FAIL |
| basic-workflow | 45% | 0 | FAIL |
| cache-efficiency | 45% | 0 | FAIL |
| cache-operations | 65% | 0 | FAIL |
| config-management | 60% | 0 | FAIL |
| context-aggregation | 30% | 0 | FAIL |
| error-recovery | 0% | 0 | FAIL |
| github-basic-workflow | 20% | 0 | FAIL |
| issue-batch-operations | 20% | 0 | FAIL |
| issue-comments-listing | 0% | 0 | FAIL |
| issue-create-advanced | 0% | 0 | FAIL |
| issue-delete | 40% | 0 | FAIL |
| issue-get-full | 40% | 0 | FAIL |
| issue-linking | 20% | 0 | FAIL |
| issue-search-templates | 0% | 0 | FAIL |
| issue-start-complete | 20% | 0 | FAIL |
| jira-basic-workflow | 20% | 0 | FAIL |
| json-output | 40% | 0 | FAIL |
| project-operations | 20% | 0 | FAIL |
| tags-operations | 20% | 0 | FAIL |
| validation-dry-run | 60% | 0 | FAIL |

**Average Score**: 27%
**Average API Calls**: 0

---

### Gemini CLI

**Provider**: `gemini`
**Evaluation Date**: 2026-02-17
**Pass Rate**: 12/21 (57%)

| Scenario | Score | API Calls | Status |
|----------|-------|-----------|--------|
| article-workflow | 90% | 11 | PASS |
| basic-workflow | 90% | 5 | PASS |
| cache-efficiency | 20% | 23 | FAIL |
| cache-operations | 85% | 8 | PASS |
| config-management | 100% | 3 | PASS |
| context-aggregation | 90% | 5 | PASS |
| error-recovery | 95% | 4 | PASS |
| github-basic-workflow | 75% | 4 | FAIL |
| issue-batch-operations | 85% | 15 | PASS |
| issue-comments-listing | 65% | 9 | FAIL |
| issue-create-advanced | 60% | 18 | FAIL |
| issue-delete | 0% | 32 | FAIL |
| issue-get-full | 0% | 30 | FAIL |
| issue-linking | 85% | 11 | PASS |
| issue-search-templates | 80% | 12 | PASS |
| issue-start-complete | 45% | 11 | FAIL |
| jira-basic-workflow | 100% | 4 | PASS |
| json-output | 100% | 4 | PASS |
| project-operations | 100% | 6 | PASS |
| tags-operations | 40% | 15 | FAIL |
| validation-dry-run | 0% | 26 | FAIL |

**Average Score**: 67%
**Average API Calls**: 12.2

---

### Other Providers

Additional providers can be added as they are tested.

---

## Scenario Descriptions

| Scenario | Description | Difficulty |
|----------|-------------|------------|
| basic-workflow | Basic issue get, update, comment flow | Easy |
| cache-efficiency | Tests efficient use of cache to minimize API calls | Medium |
| cache-operations | Cache refresh and status operations | Easy |
| config-management | Configuration verification via API | Easy |
| context-aggregation | Context command for AI session aggregation | Easy |
| error-recovery | Handling 404 errors and recovering gracefully | Medium |
| github-basic-workflow | Basic operations with GitHub backend | Easy |
| issue-batch-operations | Batch update and delete operations | Medium |
| issue-comments-listing | Adding and listing issue comments | Easy |
| issue-create-advanced | Advanced issue creation with fields, tags, subtasks | Medium |
| issue-delete | Single and batch issue deletion | Easy |
| issue-get-full | Getting issues with full context (--full flag) | Easy |
| issue-linking | Creating links between issues | Easy |
| issue-search-templates | Using search templates for common queries | Easy |
| issue-start-complete | Workflow transitions (start/complete) | Easy |
| jira-basic-workflow | Basic operations with Jira backend | Easy |
| json-output | JSON output format for machine parsing | Easy |
| project-operations | Project list, get, fields, create | Easy |
| tags-operations | Tag listing and applying tags to issues | Easy |
| validation-dry-run | Field validation for issue operations | Medium |
| article-workflow | Knowledge base article CRUD operations | Medium |

## Running Tests

```bash
# Run all scenarios with a specific provider
./scripts/run-agent-eval.sh --provider claude-code --all

# Run a single scenario
./scripts/run-agent-eval.sh --provider claude-code basic-workflow

# Run with verbose output
./scripts/run-agent-eval.sh --provider claude-code --verbose basic-workflow

# List available scenarios
./scripts/run-agent-eval.sh list
```

## Scoring

- **Base Score**: 100 points
- **Penalties**:
  - Extra commands beyond optimal: -3 to -5 points each
  - Redundant fetches: -5 to -10 points each
  - Command errors: -10 to -15 points each
- **Bonuses**:
  - Under optimal commands: +5 points
  - Cache usage: +10 points

## Notes

- Tests use a mock server that intercepts API calls and returns fixture responses
- The `--refresh` flag is required for context commands to make API calls (otherwise uses cache)
- Jira scenarios require proper environment setup (JIRA_URL, JIRA_EMAIL, JIRA_TOKEN)
