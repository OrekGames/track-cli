# Homebrew Formula for Track CLI

This directory contains the Homebrew formula template for Track CLI. The formula is automatically published to [OrekGames/homebrew-tap](https://github.com/OrekGames/homebrew-tap) by CI on each release.

## Installation

```bash
brew tap OrekGames/tap
brew install track
track init --skills   # optional: install the agent skill for Claude Code, Copilot, Cursor and Gemini CLI
```

Homebrew runs post-install steps with an isolated `HOME`, so the formula cannot
write the agent skill into your home directory; `track init --skills` does that
per user. A reference copy is installed at `$(brew --prefix)/share/track/SKILL.md`.

## Updating

```bash
brew update
brew upgrade track
```

## Uninstalling

```bash
brew uninstall track
brew untap OrekGames/tap
```

## How It Works

1. A tagged release (`v*.*.*`) triggers the [release workflow](../.github/workflows/release.yml)
2. CI builds binaries for macOS (arm64 + x86_64) and Linux (x86_64 + arm64)
3. CI generates SHA256 checksums and creates a GitHub Release
4. CI renders the formula template with the real version (written into the four release URLs, where Homebrew reads it) and checksums
5. CI pushes the updated formula to `OrekGames/homebrew-tap`

## For Maintainers

### Version Bumps

Any pull request that bumps the `track` version in `crates/track/Cargo.toml`
gets a companion pull request in [OrekGames/homebrew-tap](https://github.com/OrekGames/homebrew-tap)
that bumps `Formula/track.rb` to the same version, and each links the other.
Open it as a draft with the template's `PLACEHOLDER_SHA256_*` checksums: the
release archives only exist once the version is tagged. After the release is
published, fill in the checksums with the update script below (`--formula`
pointing at the tap's formula) and mark it ready, or close it if the release
workflow already published that version to the tap.

### Manual Formula Update

If you need to update the formula manually (outside of CI):

1. Download the checksums file from the [latest release](https://github.com/OrekGames/track-cli/releases)
2. Run the update script (requires bash 4+; on macOS `brew install bash`). It downloads all four Homebrew archives and verifies their bytes before touching the formula, refuses downgrades below the `--baseline` formula, and is idempotent for the same version. Pass the tap's current formula as the baseline, since the in-repo template's version is only a placeholder:
   ```bash
   ./scripts/update-homebrew-formula.sh --baseline /path/to/homebrew-tap/Formula/track.rb 0.2.0 dist/checksums-sha256.txt
   ```
3. Copy the updated formula to the tap repo:
   ```bash
   cp homebrew/Formula/track.rb /path/to/homebrew-tap/Formula/track.rb
   ```
4. Commit and push the tap repo

### Recovering a Failed Homebrew Update

If a release published successfully but the Homebrew update job failed, run the
[Release workflow](../.github/workflows/release.yml) manually with the
**homebrew_recovery** input enabled. This re-publishes the formula for an
existing stable release only — it does not rebuild binaries, move tags, or
create another release. Leave **version** empty to use the latest stable release.

### Troubleshooting

**Formula not found after `brew tap`:**
```bash
brew tap                         # List all taps
brew tap-info OrekGames/tap      # Show tap details
brew update                      # Refresh tap index
```

**`Refusing to load formula ... from untrusted tap`:**
Recent Homebrew versions ask you to trust third-party taps. `brew install` trusts the formula you name; for other commands, trust the tap once:
```bash
brew trust orekgames/tap
```

**Version mismatch:**
```bash
brew info track                  # Show installed version info
brew upgrade track               # Upgrade to latest
```
