---
title: Configuration
description: Config files, environment variables, and backend selection for the track CLI.
---

## Priority order

Configuration is resolved from highest to lowest priority:

1. **CLI flags** — `--url`, `--token`, `--backend`, etc.
2. **Environment variables** — backend-specific (see below)
3. **Project config** — `.track.toml` in the current directory
4. **Global config** — `~/.tracker-cli/.track.toml`

## Config file format

Create `.track.toml` in your project directory or
`~/.tracker-cli/.track.toml` for global defaults.

### YouTrack

```toml
backend = "youtrack"
url = "https://youtrack.example.com"
token = "perm:base64user.base64name.token"
default_project = "PROJ"
```

### Jira

```toml
backend = "jira"

[jira]
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"
```

### Linear

```toml
backend = "linear"
default_project = "PROJ" # Linear team key/name/id

[linear]
token = "lin_api_xxxxxxxxxxxx"
url = "https://linear.app/your-workspace" # for `track open`
# api_url = "https://api.linear.app/graphql" # default
# default_team = "PROJ"                       # optional alias for default_project
# default_linear_project = "Track CLI"        # optional issue Project association
```

### Multi-backend

You can configure multiple backends in one file and switch between them:

```toml
# Default backend
backend = "youtrack"

# YouTrack
url = "https://youtrack.example.com"
token = "perm:base64user.base64name.token"
default_project = "PROJ"

# Jira
[jira]
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"

# Optional: override link type name mappings per backend
# [jira.link_mappings]
# depends = "Requires"
```

```bash
track PROJ-123              # Uses YouTrack (default)
track -b jira PROJ-123      # Uses Jira
track -b lin ORE-123        # Uses Linear

track config backend jira   # Or change the default backend
track config backend linear
```

## Environment variables

Environment variables override config-file settings:

```bash
# Generic (any backend)
export TRACKER_BACKEND=youtrack
export TRACKER_URL=https://youtrack.example.com
export TRACKER_TOKEN=YOUR_TOKEN

# YouTrack
export YOUTRACK_URL=https://youtrack.example.com
export YOUTRACK_TOKEN=YOUR_TOKEN

# Jira
export JIRA_URL=https://your-domain.atlassian.net
export JIRA_EMAIL=you@example.com
export JIRA_TOKEN=your-api-token

# GitHub
export GITHUB_TOKEN=ghp_xxx
export GITHUB_OWNER=your-org
export GITHUB_REPO=your-repo

# GitLab
export GITLAB_TOKEN=glpat_xxx
export GITLAB_URL=https://gitlab.com/api/v4
export GITLAB_PROJECT_ID=12345

# Linear
export LINEAR_TOKEN=lin_api_xxx
export LINEAR_URL=https://linear.app/your-workspace
export LINEAR_DEFAULT_TEAM=PROJ
# export LINEAR_API_URL=https://api.linear.app/graphql
# export LINEAR_DEFAULT_PROJECT="Track CLI"
```

## Backend selection

The default backend is YouTrack. Choose a backend three ways:

### Config file (recommended)

```toml
backend = "youtrack"  # or "jira", "github", "gitlab", "linear"
```

```bash
track config backend jira
```

### Environment variable

```bash
export TRACKER_BACKEND=jira
track PROJ-123              # Uses Jira
```

### Per-command flag

```bash
track -b jira PROJ-123      # Jira for this command only
track -b yt  PROJ-123       # YouTrack
track -b lin PROJ-123       # Linear
track -b j   PROJ-123       # Jira (short alias)
```

**Priority:** CLI flag > environment variable > config file > default (YouTrack).
