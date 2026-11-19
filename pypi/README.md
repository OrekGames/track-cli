# track

`track` is a command-line tool for issue trackers: YouTrack, Jira, GitHub,
GitLab and Linear. One set of commands works across all of them, with JSON
output for scripts and AI agents.

This package installs the prebuilt `track` binary. It contains no Python
code; Python is only used to install it.

## Install

```bash
uv tool install orekgames-track
```

or

```bash
pipx install orekgames-track
```

The command is `track`:

```bash
track --version
```

To run it once without installing:

```bash
uvx --from orekgames-track track --help
```

The package is named `orekgames-track` because `track` on PyPI is an
unrelated project. `pipx install track` installs something else.

## Supported platforms

| Platform | Architectures | Requirement |
| --- | --- | --- |
| macOS | Apple Silicon, Intel | macOS 11 or newer (Intel: 10.12 or newer) |
| Windows | x64, ARM64 | |
| Linux | x86_64, aarch64 | glibc 2.39 or newer (Ubuntu 24.04, Debian 13, Fedora 40) |

On other systems the installer reports that no matching distribution was
found. See the [installation guide](https://orekgames.github.io/track-cli/installation/)
for Homebrew, the install scripts and building from source.

## Getting started

```bash
track init --url https://youtrack.example.com --token YOUR_TOKEN
track --help
```

Shell completions are not installed by this package. Generate them with
`track completions bash`, `zsh`, `fish` or `powershell`.

- Documentation: https://orekgames.github.io/track-cli/
- Source and issues: https://github.com/OrekGames/track-cli
- Each release on PyPI is built from the archives of the matching
  [GitHub release](https://github.com/OrekGames/track-cli/releases).
