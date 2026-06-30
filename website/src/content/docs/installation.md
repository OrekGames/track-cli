---
title: Installation
description: Install the track CLI via Homebrew, Cargo, or a prebuilt binary.
---

## Homebrew (macOS and Linux)

```bash
brew tap OrekGames/tap
brew install track
```

Shell completions for bash, zsh, and fish are installed automatically.

## From Source

```bash
cargo install --path crates/track
```

## Download a Binary

Download prebuilt binaries from the
[latest release](https://github.com/OrekGames/track-cli/releases). Archives are
available for macOS (arm64, x86_64), Linux (x86_64, arm64), and Windows
(x86_64, arm64).

## Shell completions

Generate completions for your shell:

```bash
track completions bash > track.bash
track completions zsh  > _track
track completions fish > track.fish
```

(Homebrew installs completions for you automatically.)
