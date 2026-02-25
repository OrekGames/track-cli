# Homebrew Formula for Track CLI

This directory contains the Homebrew formula template for Track CLI. The formula is automatically published to [OrekGames/homebrew-tap](https://github.com/OrekGames/homebrew-tap) by CI on each release.

## Installation

```bash
brew tap OrekGames/tap
brew install track
```

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
4. CI renders the formula template with real version and checksums
5. CI pushes the updated formula to `OrekGames/homebrew-tap`

## For Maintainers

### Manual Formula Update

If you need to update the formula manually (outside of CI):

1. Download the checksums file from the [latest release](https://github.com/OrekGames/track-cli/releases)
2. Run the update script:
   ```bash
   ./scripts/update-homebrew-formula.sh 0.2.0 dist/checksums-sha256.txt
   ```
3. Copy the updated formula to the tap repo:
   ```bash
   cp homebrew/Formula/track.rb /path/to/homebrew-tap/Formula/track.rb
   ```
4. Commit and push the tap repo

### Troubleshooting

**Formula not found after `brew tap`:**
```bash
brew tap                         # List all taps
brew tap-info OrekGames/tap      # Show tap details
brew update                      # Refresh tap index
```

**Version mismatch:**
```bash
brew info track                  # Show installed version info
brew upgrade track               # Upgrade to latest
```
