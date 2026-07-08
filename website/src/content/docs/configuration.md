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

# Link type mappings are covered below.
```

```bash
track PROJ-123              # Uses YouTrack (default)
track -b jira PROJ-123      # Uses Jira
track -b lin ORE-123        # Uses Linear

track config backend jira   # Or change the default backend
track config backend linear
```

### Link type mappings

`track issue link` accepts a small set of canonical link names so commands can
stay portable across trackers:

`relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, and
`parent`.

Backends then translate those names into the native link type expected by the
remote API. This matters when your tracker admins rename issue link types, add
custom relationship names, or use different words for the same workflow. For
example, one Jira instance might call a dependency link `Blocks`, while another
calls it `Requires`.

`subtask` and `parent` use backend-native hierarchy APIs where available. Link
type mappings apply to the general issue-link path used by `relates`, `depends`,
`required`, `duplicates`, `duplicated-by`, and any custom type name you pass with
`-t`.

#### Defaults

| CLI link type | YouTrack | Jira | GitLab | Linear |
| --- | --- | --- | --- | --- |
| `relates` | `Relates` | `Relates` | `relates_to` | `related` |
| `depends` | `Depend` | `Blocks` | `blocks` | `blocks` |
| `required` | `Depend` | `Blocks` | `is_blocked_by` | `blocks` |
| `duplicates` | `Duplicate` | `Duplicate` | `relates_to` | `duplicate` |
| `duplicated-by` | `Duplicate` | `Duplicate` | `relates_to` | `duplicate` |

#### Override mappings

Add a backend-specific `link_mappings` table to `.track.toml`. Keys are the CLI
link type names you want to use; values are the native backend names.

```toml
[jira.link_mappings]
depends = "Requires"
required = "Requires"
duplicates = "Cloners"

[youtrack.link_mappings]
depends = "Is required for"

[gitlab.link_mappings]
duplicates = "blocks"

[linear.link_mappings]
relates = "similar"
```

With that config, agents and scripts keep using the same command shape:

```bash
track -b j  i link PROJ-10 PROJ-11 -t depends
track -b yt i link PROJ-10 PROJ-11 -t depends
track -b gl i link 42 43 -t duplicates
track -b lin i link ORE-10 ORE-11 -t relates
```

You can also create local aliases for custom relationship names. Unrecognized
types are passed through by default, but a mapping lets your team expose a stable
CLI vocabulary even if the backend's native name is awkward:

```toml
[jira.link_mappings]
qa-blocker = "Blocks"

[linear.link_mappings]
design-related = "similar"
```

```bash
track -b j   i link PROJ-20 PROJ-21 -t qa-blocker
track -b lin i link ORE-20 ORE-21 -t design-related
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
