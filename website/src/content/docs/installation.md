---
title: Installation
description: Install the track CLI with Homebrew, the native installer, Cargo, or a prebuilt binary.
---

## Homebrew (Package Manager)

```bash
brew tap OrekGames/tap
brew install track
```

Shell completions for bash, zsh, and fish are installed automatically.

## Native Install

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/OrekGames/track-cli/main/scripts/install.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/OrekGames/track-cli/main/scripts/install.ps1 | iex
```

The native installers download the latest GitHub release archive, verify it with
`checksums-sha256.txt`, install `track` into a user-owned directory, and install
shell completions where supported.

Override the install directory with `TRACK_INSTALL_DIR`, or set
`TRACK_SKIP_PATH=1` to skip shell startup file or user PATH changes.

Pin a release with `TRACK_VERSION`:

```bash
curl -fsSL https://raw.githubusercontent.com/OrekGames/track-cli/v1.15.1/scripts/install.sh | TRACK_VERSION=1.15.1 bash
```

```powershell
$env:TRACK_VERSION = "1.15.1"; irm https://raw.githubusercontent.com/OrekGames/track-cli/v1.15.1/scripts/install.ps1 | iex
```

Agent skills are optional and installed explicitly after the CLI is available:

```bash
track init --skills
```

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
