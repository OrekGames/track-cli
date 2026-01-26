# typed: false
# frozen_string_literal: true

# Homebrew formula for Track CLI
# A CLI for issue tracking systems (YouTrack, etc.)
#
# Installation:
#   1. Set GITLAB_TOKEN environment variable with read_api scope
#   2. brew tap your-group/track /path/to/homebrew-track
#   3. brew install track

class Track < Formula
  desc "CLI for issue tracking systems (YouTrack, Jira, etc.)"
  homepage "https://gitlab.com/your-group/youtrack-cli"
  version "0.1.0"
  license "MIT"

  # GitLab Package Registry URLs - requires GITLAB_TOKEN for private repos
  # Replace YOUR_PROJECT_ID with the actual GitLab project ID
  GITLAB_PROJECT_ID = "YOUR_PROJECT_ID"
  GITLAB_API_URL = "https://gitlab.com/api/v4"

  on_macos do
    on_arm do
      url "#{GITLAB_API_URL}/projects/#{GITLAB_PROJECT_ID}/packages/generic/track/#{version}/track-#{version}-aarch64-apple-darwin.tar.gz",
          headers: ["PRIVATE-TOKEN: #{ENV.fetch("GITLAB_TOKEN", nil)}"]
      sha256 "PLACEHOLDER_SHA256_ARM64"
    end

    on_intel do
      url "#{GITLAB_API_URL}/projects/#{GITLAB_PROJECT_ID}/packages/generic/track/#{version}/track-#{version}-x86_64-apple-darwin.tar.gz",
          headers: ["PRIVATE-TOKEN: #{ENV.fetch("GITLAB_TOKEN", nil)}"]
      sha256 "PLACEHOLDER_SHA256_X86_64"
    end
  end

  on_linux do
    on_arm do
      url "#{GITLAB_API_URL}/projects/#{GITLAB_PROJECT_ID}/packages/generic/track/#{version}/track-#{version}-aarch64-unknown-linux-gnu.tar.gz",
          headers: ["PRIVATE-TOKEN: #{ENV.fetch("GITLAB_TOKEN", nil)}"]
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"
    end

    on_intel do
      url "#{GITLAB_API_URL}/projects/#{GITLAB_PROJECT_ID}/packages/generic/track/#{version}/track-#{version}-x86_64-unknown-linux-gnu.tar.gz",
          headers: ["PRIVATE-TOKEN: #{ENV.fetch("GITLAB_TOKEN", nil)}"]
      sha256 "PLACEHOLDER_SHA256_LINUX_X86_64"
    end
  end

  def install
    bin.install "track"

    # Install shell completions if present
    bash_completion.install "track.bash" => "track" if File.exist?("track.bash")
    zsh_completion.install "_track" if File.exist?("_track")
    fish_completion.install "track.fish" if File.exist?("track.fish")
  end

  def caveats
    <<~EOS
      To use track, you need to configure your tracker credentials.

      Set environment variables:
        export TRACKER_URL="https://your-youtrack-instance.com"
        export TRACKER_TOKEN="your-api-token"

      Or create a config file at ~/.config/track/config.toml:
        [youtrack]
        url = "https://your-youtrack-instance.com"
        token = "your-api-token"

      For more information, see:
        track --help
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/track --version")
  end
end
