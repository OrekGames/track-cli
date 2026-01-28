#!/usr/bin/env bash
#
# run-agent-eval.sh - Evaluate AI agent performance using track-agent harness
#
# This script wraps the track-agent binary for convenient evaluation of
# AI agents (Anthropic API or Claude Code CLI) against mock scenarios.
#
# Usage:
#   ./scripts/run-agent-eval.sh [options] [scenario]
#
# Examples:
#   ./scripts/run-agent-eval.sh list                    # List available scenarios
#   ./scripts/run-agent-eval.sh basic-workflow          # Run single scenario
#   ./scripts/run-agent-eval.sh --all                   # Run all scenarios
#   ./scripts/run-agent-eval.sh --provider claude-code basic-workflow
#   ./scripts/run-agent-eval.sh --verbose --json cache-efficiency

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Defaults
SCENARIOS_DIR="$PROJECT_ROOT/fixtures/scenarios"
PROVIDER="anthropic"
MAX_TURNS=20
MIN_SCORE=70
VERBOSE=""
FORMAT="text"
FAIL_FAST=""
RUN_ALL=""
MODEL=""

# Binary path
TRACK_AGENT="$PROJECT_ROOT/target/release/track-agent"

usage() {
    cat << EOF
Usage: $(basename "$0") [options] [scenario|command]

Commands:
  list              List all available scenarios
  show <scenario>   Show details for a scenario
  <scenario>        Run a specific scenario (by name or path)

Options:
  -a, --all              Run all scenarios
  -p, --provider <name>  Provider: anthropic (default) or claude-code
  -m, --model <model>    Model to use (default: claude-sonnet-4-20250514)
  -t, --turns <n>        Maximum turns (default: 20)
  -s, --min-score <n>    Minimum passing score (default: 70)
  -v, --verbose          Show verbose output (all messages)
  -j, --json             Output results as JSON
  -f, --fail-fast        Stop on first failure (with --all)
  --scenarios <dir>      Path to scenarios directory
  -h, --help             Show this help message

Environment Variables:
  ANTHROPIC_API_KEY      Required for anthropic provider

Examples:
  # List scenarios
  $(basename "$0") list

  # Run a single scenario with Anthropic API
  $(basename "$0") basic-workflow

  # Run with Claude Code CLI
  $(basename "$0") --provider claude-code basic-workflow

  # Run all scenarios with JSON output
  $(basename "$0") --all --json

  # Run with verbose output and custom model
  $(basename "$0") -v --model claude-opus-4-20250514 cache-efficiency

  # Run all and stop on first failure
  $(basename "$0") --all --fail-fast

EOF
}

log_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1" >&2
}

# Build the binary if needed
ensure_binary() {
    if [[ ! -x "$TRACK_AGENT" ]]; then
        log_info "Building track-agent (release mode)..."
        cargo build --release --package agent-harness -q
        if [[ ! -x "$TRACK_AGENT" ]]; then
            log_error "Failed to build track-agent"
            exit 1
        fi
        log_success "Built track-agent"
    fi
}

# Validate API key for anthropic provider
check_api_key() {
    if [[ "$PROVIDER" == "anthropic" && -z "${ANTHROPIC_API_KEY:-}" ]]; then
        log_error "ANTHROPIC_API_KEY environment variable is required for anthropic provider"
        echo ""
        echo "Set it with:"
        echo "  export ANTHROPIC_API_KEY=your-api-key"
        echo ""
        echo "Or use Claude Code CLI instead:"
        echo "  $(basename "$0") --provider claude-code $*"
        exit 1
    fi
}

# Resolve scenario path from name
resolve_scenario() {
    local name="$1"

    # If it's already a path, use it directly
    if [[ -d "$name" ]]; then
        echo "$name"
        return
    fi

    # Check in scenarios directory
    local path="$SCENARIOS_DIR/$name"
    if [[ -d "$path" ]]; then
        echo "$path"
        return
    fi

    log_error "Scenario not found: $name"
    echo ""
    echo "Available scenarios:"
    list_scenarios_brief
    exit 1
}

list_scenarios_brief() {
    for dir in "$SCENARIOS_DIR"/*/; do
        if [[ -f "$dir/scenario.toml" ]]; then
            basename "$dir"
        fi
    done
}

# Parse arguments
POSITIONAL_ARGS=()
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            usage
            exit 0
            ;;
        -a|--all)
            RUN_ALL="1"
            shift
            ;;
        -p|--provider)
            PROVIDER="$2"
            shift 2
            ;;
        -m|--model)
            MODEL="$2"
            shift 2
            ;;
        -t|--turns)
            MAX_TURNS="$2"
            shift 2
            ;;
        -s|--min-score)
            MIN_SCORE="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE="--verbose"
            shift
            ;;
        -j|--json)
            FORMAT="json"
            shift
            ;;
        -f|--fail-fast)
            FAIL_FAST="--fail-fast"
            shift
            ;;
        --scenarios)
            SCENARIOS_DIR="$2"
            shift 2
            ;;
        -*)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
        *)
            POSITIONAL_ARGS+=("$1")
            shift
            ;;
    esac
done

set -- "${POSITIONAL_ARGS[@]:-}"

# Main logic
ensure_binary

# Handle commands
if [[ ${#POSITIONAL_ARGS[@]} -eq 0 && -z "$RUN_ALL" ]]; then
    usage
    exit 0
fi

COMMAND="${POSITIONAL_ARGS[0]:-}"

case "$COMMAND" in
    list)
        exec "$TRACK_AGENT" list --path "$SCENARIOS_DIR"
        ;;
    show)
        if [[ ${#POSITIONAL_ARGS[@]} -lt 2 ]]; then
            log_error "Missing scenario name for 'show' command"
            exit 1
        fi
        SCENARIO_PATH=$(resolve_scenario "${POSITIONAL_ARGS[1]}")
        exec "$TRACK_AGENT" show "$SCENARIO_PATH"
        ;;
    *)
        # Build command arguments
        CMD_ARGS=()

        if [[ -n "$RUN_ALL" ]]; then
            # Run all scenarios
            CMD_ARGS+=("run-all")
            CMD_ARGS+=("--path" "$SCENARIOS_DIR")
            [[ -n "$FAIL_FAST" ]] && CMD_ARGS+=("$FAIL_FAST")
        else
            # Run single scenario
            if [[ -z "$COMMAND" ]]; then
                log_error "No scenario specified"
                usage
                exit 1
            fi
            SCENARIO_PATH=$(resolve_scenario "$COMMAND")
            CMD_ARGS+=("run" "$SCENARIO_PATH")
            [[ -n "$VERBOSE" ]] && CMD_ARGS+=("$VERBOSE")
        fi

        # Common options
        CMD_ARGS+=("--provider" "$PROVIDER")
        CMD_ARGS+=("--max-turns" "$MAX_TURNS")
        CMD_ARGS+=("--min-score" "$MIN_SCORE")
        CMD_ARGS+=("--format" "$FORMAT")
        [[ -n "$MODEL" ]] && CMD_ARGS+=("--model" "$MODEL")

        # Check API key for anthropic
        check_api_key

        # Run the command
        if [[ "$FORMAT" == "text" ]]; then
            echo ""
            if [[ -n "$RUN_ALL" ]]; then
                log_info "Running all scenarios with $PROVIDER provider..."
            else
                log_info "Running scenario: $(basename "$SCENARIO_PATH")"
            fi
            log_info "Provider: $PROVIDER | Max turns: $MAX_TURNS | Min score: $MIN_SCORE"
            echo ""
        fi

        exec "$TRACK_AGENT" "${CMD_ARGS[@]}"
        ;;
esac
