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
#   5. Installs Claude Code skill to ~/.claude/skills/track/
#   6. Creates a global config template if none exists
#   7. Adds the directory to PATH in shell config files

set -euo pipefail

INSTALL_DIR="$HOME/.tracker-cli"
DOCS_DIR="$INSTALL_DIR/docs"
SKILL_DIR="$HOME/.claude/skills/track"
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

# Install Claude Code skill
echo "Installing Claude Code skill to $SKILL_DIR"
mkdir -p "$SKILL_DIR"
cp "$PROJECT_DIR/.claude/skills/track/SKILL.md" "$SKILL_DIR/SKILL.md"

# Create global config template if it doesn't exist
GLOBAL_CONFIG="$INSTALL_DIR/.track.toml"
if [[ ! -f "$GLOBAL_CONFIG" ]]; then
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
    echo "Global config already exists: $GLOBAL_CONFIG (skipped)"
fi

# Function to add PATH to a shell config file
add_to_path() {
    local config_file="$1"
    local path_line="export PATH=\"\$HOME/.tracker-cli:\$PATH\""

    if [[ -f "$config_file" ]]; then
        if grep -q ".tracker-cli" "$config_file" 2>/dev/null; then
            echo "  $config_file: PATH already configured"
        else
            echo "" >> "$config_file"
            echo "# Added by track CLI installer" >> "$config_file"
            echo "$path_line" >> "$config_file"
            echo "  $config_file: PATH added"
        fi
    fi
}

echo ""
echo "Configuring shell PATH..."

# Check for common shell config files
if [[ -f "$HOME/.zshrc" ]]; then
    add_to_path "$HOME/.zshrc"
elif [[ "$SHELL" == *"zsh"* ]]; then
    # Create .zshrc if user's shell is zsh but file doesn't exist
    touch "$HOME/.zshrc"
    add_to_path "$HOME/.zshrc"
fi

if [[ -f "$HOME/.bashrc" ]]; then
    add_to_path "$HOME/.bashrc"
fi

if [[ -f "$HOME/.bash_profile" ]]; then
    add_to_path "$HOME/.bash_profile"
fi

echo ""
echo "Installation complete!"
echo ""
echo "Installed files:"
echo "  Binary:  $INSTALL_DIR/$BINARY_NAME"
echo "  Docs:    $DOCS_DIR/README.md"
echo "           $DOCS_DIR/agent_guide.md"
echo "  Skill:   $SKILL_DIR/SKILL.md"
echo "  Config:  $GLOBAL_CONFIG"
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
echo ""
echo "View documentation:"
echo "  cat $DOCS_DIR/README.md"
echo "  cat $DOCS_DIR/agent_guide.md"
