# typed: false
# frozen_string_literal: true

# Homebrew formula for Track CLI
# A CLI for issue tracking systems (YouTrack, Jira, GitHub, GitLab)
#
# Installation:
#   brew tap OrekGames/tap
#   brew install track
#
# The version is read from the release URLs, so they carry it literally;
# scripts/update-homebrew-formula.sh rewrites the URLs and checksums.

class Track < Formula
  desc "CLI for issue tracking systems (YouTrack, Jira, GitHub, GitLab)"
  homepage "https://github.com/OrekGames/track-cli"
  license "MIT"

  livecheck do
    url :homepage
    strategy :github_latest
  end

  on_macos do
    on_arm do
      url "https://github.com/OrekGames/track-cli/releases/download/v0.1.0/track-0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_ARM64"
    end

    on_intel do
      url "https://github.com/OrekGames/track-cli/releases/download/v0.1.0/track-0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/OrekGames/track-cli/releases/download/v0.1.0/track-0.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"
    end

    on_intel do
      url "https://github.com/OrekGames/track-cli/releases/download/v0.1.0/track-0.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  def install
    bin.install "track"

    # Install shell completions if present in the archive
    bash_completion.install "track.bash" => "track" if File.exist?("track.bash")
    zsh_completion.install "_track" if File.exist?("_track")
    fish_completion.install "track.fish" if File.exist?("track.fish")

    # Install documentation
    doc.install "README.md" if File.exist?("README.md")
    doc.install "agent_guide.md" if File.exist?("agent_guide.md")

    # Keep a reference copy of the agent skill. Homebrew runs post-install
    # with an isolated HOME, so the skill is installed per user by
    # `track init --skills` (see caveats) rather than by the formula.
    pkgshare.install "agent-skills/SKILL.md" if File.exist?("agent-skills/SKILL.md")
  end

  def caveats
    <<~EOS
      To use track, configure your tracker credentials:

        track init --url https://your-instance.com --token YOUR_TOKEN

      Or set environment variables:
        export TRACKER_URL="https://your-tracker-instance.com"
        export TRACKER_TOKEN="your-api-token"

      To install the track skill for Claude Code, Copilot, Cursor, and Gemini CLI
      (~/.claude/skills/track/, ~/.copilot/skills/track/,
       ~/.cursor/skills/track/, ~/.gemini/skills/track/), run:

        track init --skills

      For more information, see:
        track --help
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/track --version")
  end
end
