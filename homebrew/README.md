# Homebrew Tap for Track CLI

This directory contains the Homebrew formula for installing Track CLI.

## Setup (Private GitLab Repository)

Since this is a private GitLab repository, you need to configure authentication before installing.

### 1. Create a GitLab Personal Access Token

1. Go to GitLab → Settings → Access Tokens
2. Create a token with `read_api` scope
3. Save the token securely

### 2. Set Environment Variable

Add to your `~/.zshrc` or `~/.bashrc`:

```bash
export GITLAB_TOKEN="glpat-xxxxxxxxxxxxxxxxxxxx"
```

Reload your shell:
```bash
source ~/.zshrc  # or ~/.bashrc
```

### 3. Add the Tap

```bash
# Clone the tap (or use the homebrew-track repo if separate)
brew tap your-group/track https://gitlab.com/your-group/youtrack-cli.git --force-auto-update
```

### 4. Install Track

```bash
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
brew untap your-group/track
```

## Troubleshooting

### "401 Unauthorized" Error

Make sure your `GITLAB_TOKEN` is set and has `read_api` scope:

```bash
echo $GITLAB_TOKEN  # Should print your token
```

### "404 Not Found" Error

The release may not exist yet. Check the GitLab releases page to verify the version exists.

### Formula Not Found

Ensure the tap was added correctly:

```bash
brew tap  # List all taps
brew tap-info your-group/track  # Show tap details
```

## For Maintainers

### Updating the Formula

After creating a new release:

1. Download the checksums file from the release
2. Run the update script:
   ```bash
   ./scripts/update-homebrew-formula.sh 0.2.0 dist/checksums-sha256.txt
   ```
3. Commit and push the updated formula

### Manual Checksum Update

If needed, update `homebrew/Formula/track.rb` manually:

1. Update the `version` line
2. Update each `sha256` line with the correct checksum from `checksums-sha256.txt`
