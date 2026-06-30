---
title: Quick Start
description: Configure a backend and run your first track commands.
---

## 1. Initialize configuration

Create a `.track.toml` in your project directory (or
`~/.config/track/config.toml` for global config):

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

## 2. Set a default project (optional)

```bash
track config project PROJ
```

## 3. Test the connection

```bash
track config test
```

## 4. Basic usage

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
