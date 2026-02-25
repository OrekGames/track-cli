#!/bin/bash
# Build and install the track CLI locally
#
# Usage: ./scripts/install-local.sh
#
# This script:
#   1. Builds the project in release mode
#   2. Creates ~/.tracker-cli directory
#   3. Copies the track binary there
#   4. Copies documentation (README.md, agent_guide.md)
#   5. Generates and installs shell completions
#   6. Installs agent skills for Claude Code, Gemini CLI, Copilot, and Cursor
#   7. Creates a global config template if none exists
#   8. Adds the directory to PATH in shell config files

set -euo pipefail

INSTALL_DIR="$HOME/.tracker-cli"
DOCS_DIR="$INSTALL_DIR/docs"
COMPLETIONS_DIR="$INSTALL_DIR/completions"
BINARY_NAME="track"

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "Building track CLI in release mode..."
cargo build --release

echo "Creating install directory: $INSTALL_DIR"
mkdir -p "$INSTALL_DIR"
mkdir -p "$DOCS_DIR"

echo "Copying binary to $INSTALL_DIR/$BINARY_NAME"
cp "target/release/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

# On macOS, re-sign the binary to fix code signature after copying
if [[ "$(uname)" == "Darwin" ]]; then
    echo "Re-signing binary for macOS..."
    codesign --force --sign - "$INSTALL_DIR/$BINARY_NAME"
fi

echo "Copying documentation to $DOCS_DIR"
cp "$PROJECT_DIR/README.md" "$DOCS_DIR/README.md"
cp "$PROJECT_DIR/docs/agent_guide.md" "$DOCS_DIR/agent_guide.md"

# Generate and install shell completions
echo "Generating shell completions..."
mkdir -p "$COMPLETIONS_DIR"
"$INSTALL_DIR/$BINARY_NAME" completions bash > "$COMPLETIONS_DIR/track.bash"
"$INSTALL_DIR/$BINARY_NAME" completions zsh  > "$COMPLETIONS_DIR/_track"
"$INSTALL_DIR/$BINARY_NAME" completions fish > "$COMPLETIONS_DIR/track.fish"

# Install agent skill globally for all supported AI coding tools
SKILL_SRC="$PROJECT_DIR/agent-skills/SKILL.md"
echo ""
echo "Installing agent skills for AI coding tools..."

for TOOL_DIR in .claude .copilot .cursor .gemini; do
    TOOL_SKILL_DIR="$HOME/$TOOL_DIR/skills/track"
    mkdir -p "$TOOL_SKILL_DIR"
    cp "$SKILL_SRC" "$TOOL_SKILL_DIR/SKILL.md"
    echo "  $TOOL_SKILL_DIR/SKILL.md"
done

# Create global config template if it doesn't exist
GLOBAL_CONFIG="$INSTALL_DIR/.track.toml"
if [[ ! -f "$GLOBAL_CONFIG" ]]; then
    echo ""
    echo "Creating global config template: $GLOBAL_CONFIG"
    cat > "$GLOBAL_CONFIG" << 'TOML'
# Track CLI - Global Configuration
# This file provides default settings when no local .track.toml exists.
# Local project configs (.track.toml in project dir) override these values.
#
# Uncomment and fill in the values for your setup.

# Default backend: "youtrack" or "jira"
# backend = "youtrack"

# Global settings (used by whichever backend is active)
# url = "https://youtrack.example.com"
# token = "perm:xxx"
# email = ""
# default_project = "PROJ"

# YouTrack-specific settings
# [youtrack]
# url = "https://youtrack.example.com"
# token = "perm:xxx"

# Jira-specific settings
# [jira]
# url = "https://your-domain.atlassian.net"
# email = "you@example.com"
# token = "your-api-token"
TOML
else
    echo ""
    echo "Global config already exists: $GLOBAL_CONFIG (skipped)"
fi

# Function to add PATH and completions to a shell config file
add_to_shell_config() {
    local config_file="$1"
    local shell_type="$2"

    if [[ ! -f "$config_file" ]]; then
        return
    fi

    # Add PATH
    local path_line="export PATH=\"\$HOME/.tracker-cli:\$PATH\""
    if grep -q ".tracker-cli" "$config_file" 2>/dev/null; then
        echo "  $config_file: PATH already configured"
    else
        echo "" >> "$config_file"
        echo "# Added by track CLI installer" >> "$config_file"
        echo "$path_line" >> "$config_file"
        echo "  $config_file: PATH added"
    fi

    # Add completions
    if ! grep -q "tracker-cli/completions" "$config_file" 2>/dev/null; then
        if [[ "$shell_type" == "zsh" ]]; then
            echo "fpath=(\$HOME/.tracker-cli/completions \$fpath)" >> "$config_file"
            echo "  $config_file: zsh completions added (run 'compinit' or restart shell)"
        elif [[ "$shell_type" == "bash" ]]; then
            echo "source \"\$HOME/.tracker-cli/completions/track.bash\"" >> "$config_file"
            echo "  $config_file: bash completions added"
        fi
    else
        echo "  $config_file: completions already configured"
    fi
}

echo ""
echo "Configuring shell PATH and completions..."

# Check for common shell config files
if [[ -f "$HOME/.zshrc" ]]; then
    add_to_shell_config "$HOME/.zshrc" "zsh"
elif [[ "$SHELL" == *"zsh"* ]]; then
    # Create .zshrc if user's shell is zsh but file doesn't exist
    touch "$HOME/.zshrc"
    add_to_shell_config "$HOME/.zshrc" "zsh"
fi

if [[ -f "$HOME/.bashrc" ]]; then
    add_to_shell_config "$HOME/.bashrc" "bash"
fi

if [[ -f "$HOME/.bash_profile" ]]; then
    add_to_shell_config "$HOME/.bash_profile" "bash"
fi

# Install fish completions via symlink if fish is present
if [[ -d "$HOME/.config/fish" ]]; then
    FISH_COMPLETIONS_DIR="$HOME/.config/fish/completions"
    mkdir -p "$FISH_COMPLETIONS_DIR"
    ln -sf "$COMPLETIONS_DIR/track.fish" "$FISH_COMPLETIONS_DIR/track.fish"
    echo "  fish: completions symlinked to $FISH_COMPLETIONS_DIR/track.fish"
fi

echo ""
echo "Installation complete!"
echo ""
echo "Installed files:"
echo "  Binary:       $INSTALL_DIR/$BINARY_NAME"
echo "  Docs:         $DOCS_DIR/README.md"
echo "                $DOCS_DIR/agent_guide.md"
echo "  Completions:  $COMPLETIONS_DIR/ (bash, zsh, fish)"
echo "  Config:       $GLOBAL_CONFIG"
echo ""
echo "Agent skills (installed globally for all AI coding tools):"
for TOOL_DIR in .claude .copilot .cursor .gemini; do
    echo "  ~/$TOOL_DIR/skills/track/SKILL.md"
done
echo ""
echo "To use the 'track' command immediately, run one of:"
echo "  source ~/.zshrc    # for zsh"
echo "  source ~/.bashrc   # for bash"
echo ""
echo "Or open a new terminal window."
echo ""
echo "Configure your global defaults:"
echo "  \$EDITOR $GLOBAL_CONFIG"
echo ""
echo "Verify installation:"
echo "  track --version"
