---
title: Quick Start
description: Configure a backend and run your first track commands.
---

## 1. Initialize configuration

Create a `.track.toml` in your project directory (or
`~/.tracker-cli/.track.toml` for global config):

Project configs can contain API tokens. For local init, `track init` updates an
existing `.gitignore` with `.track.toml` and `.tracker-cache/`. If your project
doesn't have a `.gitignore` yet, add those entries before committing.

```bash
# YouTrack (default)
track init --url https://youtrack.example.com --token YOUR_TOKEN

# Jira
track init --url https://your-domain.atlassian.net --token YOUR_TOKEN \
  --backend jira --email you@example.com

# Linear (URL is the workspace URL used by `track open`)
track init --url https://linear.app/your-workspace --token YOUR_LINEAR_API_KEY \
  --backend linear --project PROJ
```

## 2. Install agent skills (recommended for AI sessions)

`track init --skills` installs the bundled `track` skill reference so AI
coding assistants know the command aliases, JSON output mode, cache/context
workflow, backend quirks, and safe batch patterns.

```bash
track init --skills           # Install skills only; no tracker config change
track init --skills --url ... # Combine with configuration initialization
```

Supported agent destinations:

| Agent | Installed path |
| --- | --- |
| Claude Code | `~/.claude/skills/track/SKILL.md` |
| GitHub Copilot | `~/.copilot/skills/track/SKILL.md` |
| Cursor | `~/.cursor/skills/track/SKILL.md` |
| Gemini CLI | `~/.gemini/skills/track/SKILL.md` |

The skill files do not store tracker credentials. They are documentation for
agents; `.track.toml` remains the config file that contains tokens and backend
settings.

## 3. Set a default project (optional)

```bash
track config project PROJ
```

## 4. Test the connection

```bash
track config test    # Quick URL/token check
track doctor         # Deeper capability audit
```

`config test` proves the configured backend can answer a basic connectivity
probe. `doctor` checks the practical capabilities you will rely on: search,
issue reads, comments, links, field schema, field admin, articles, and optional
local write validation.

## 5. Basic usage

```bash
track PROJ-123              # Get an issue (shortcut for `issue get`)
track PROJ-123 --full       # With comments, links, and subtasks
track open PROJ-123         # Open in browser

track i s "project: PROJ #Unresolved"          # Search
track i new -p PROJ -s "New issue"             # Create
track i u PROJ-123 --field "Priority=Critical" # Update
```

Add `-o json` (or `--format json`) to any command for machine-readable output:

```bash
track -o json PROJ-123
```

Next: see [Configuration](/track-cli/configuration/) for the full set of options,
or the [Commands](/track-cli/commands/) reference.
