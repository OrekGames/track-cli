# Track CLI

[![CI](https://github.com/OrekGames/track-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/OrekGames/track-cli/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/OrekGames/track-cli)](https://github.com/OrekGames/track-cli/releases/latest)
[![Homebrew](https://img.shields.io/badge/homebrew-OrekGames%2Ftap-orange)](https://github.com/OrekGames/homebrew-tap)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A command-line interface for issue tracking systems, built with Rust. Supports **YouTrack**, **Jira**, **GitHub**, **GitLab**, and **Linear** with a unified command interface.

## Features

- **Multi-Backend**: YouTrack, Jira, GitHub, GitLab, and Linear with the same commands
- **Issue Management**: Get, create, update, delete, search issues
- **Batch Operations**: Update, delete, or complete multiple issues at once
- **Transparent Pagination**: `--all` flag auto-paginates to fetch every result
- **Custom Fields**: Set priority, state, assignee, and any field with validation
- **Field Admin**: Create custom fields and bundles, attach to projects (YouTrack)
- **Comments & Links**: Add comments and link issues together
- **Knowledge Base**: Manage articles (YouTrack and Jira/Confluence)
- **AI-Optimized**: Context aggregation, query templates, workflow hints
- **Output Formats**: Text (human-readable), pretty JSON, and compact JSON Lines (machine-readable)
- **Flexible Config**: CLI flags, environment variables, or config file

## Installation

### Native Install

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
shell completions where supported. Override the install directory with
`TRACK_INSTALL_DIR`, or set `TRACK_SKIP_PATH=1` to skip shell startup file or
user PATH changes. Pin a release with `TRACK_VERSION`:

```bash
curl -fsSL https://raw.githubusercontent.com/OrekGames/track-cli/v1.15.1/scripts/install.sh | TRACK_VERSION=1.15.1 bash
```

```powershell
$env:TRACK_VERSION = "1.15.1"; irm https://raw.githubusercontent.com/OrekGames/track-cli/v1.15.1/scripts/install.ps1 | iex
```

Agent setup is optional and done explicitly after the CLI is available.
`track agent install` sets up Orek.ai, Claude Code, Codex, Cursor, Antigravity,
Gemini CLI and Copilot: the bundled `track` skill, and, where the tool supports
one, a session-start hook that prints the project briefing:

```bash
track agent install
```

### Homebrew (Package Manager)

```bash
brew tap OrekGames/tap
brew install track
```

Shell completions for bash, zsh, and fish are installed automatically.

To set up AI coding agents (Orek.ai, Claude Code, Codex, Cursor, Antigravity, Gemini CLI
and Copilot), run `track agent install` (Homebrew can't write into your home directory
itself).

### From Source

```bash
cargo install --path crates/track
```

### Download Binary

Download prebuilt binaries from the [latest release](https://github.com/OrekGames/track-cli/releases). Archives are available for macOS (arm64, x86_64), Linux (x86_64, arm64), and Windows (x86_64, arm64).

## Quick Start

### 1. Initialize Configuration

Create a `.track.toml` file in your project directory or `~/.tracker-cli/.track.toml` for global configuration.
Project configs can contain API tokens. If a `.gitignore` exists in the current directory, local `track init` adds `.track.toml` and `.tracker-cache/` automatically; otherwise add those entries before committing:

#### GitHub

Create a [GitHub personal access token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens)
that can access issues in the target repository. Replace
`YOUR_GITHUB_TOKEN` and `owner/repo` below; `owner/repo` is the repository
path, for example `OrekGames/track-cli`.

```bash
track init \
  --backend github \
  --url https://api.github.com \
  --token YOUR_GITHUB_TOKEN \
  --project owner/repo

track i s "is:open"
```

`track init` validates the repository and writes a local `.track.toml`; the
next command searches open issues using that saved GitHub configuration. See
the [GitHub backend notes](https://orekgames.github.io/track-cli/backends/#github)
and [configuration reference](https://orekgames.github.io/track-cli/configuration/)
for backend behavior and other configuration methods.

```bash
# Initialize with YouTrack (default)
track init --url https://youtrack.example.com --token YOUR_TOKEN

# Or initialize with Jira
track init --url https://your-domain.atlassian.net --token YOUR_TOKEN --backend jira --email you@example.com

# Or initialize with Linear (URL is the Linear workspace URL used by `track open`)
track init --url https://linear.app/your-workspace --token YOUR_LINEAR_API_KEY --backend linear --project PROJ
```

For AI-assisted workflows, set up agents once per user:

```bash
track agent install
```

This sets up Orek.ai, Claude Code, Codex, Cursor, Antigravity, Gemini CLI and Copilot.

### 2. Set Default Project (Optional)

```bash
track config project PROJ
```

### 3. Test Connection

```bash
track config test    # Quick connectivity check (URL + token)
track doctor         # Deeper capability audit (search, reads, fields, articles, ...)
```

### 4. Basic Usage

```bash
# Get an issue
track PROJ-123

# Search issues
track issue search "project: PROJ #Unresolved" --limit 10

# Create an issue
track issue create -p PROJ -s "Fix bug" -d "Description"
```

## Configuration

Configuration priority order (highest to lowest):

1. **CLI flags**: `--url`, `--token`, `--backend`, etc.
2. **Environment variables**: Backend-specific (see below)
3. **Project config**: `.track.toml` in current directory
4. **Global config**: `~/.config/track/config.toml`

### Config File Format

Create `.track.toml` in your project directory or `~/.config/track/config.toml`:

#### YouTrack Configuration

```toml
# .track.toml
backend = "youtrack"
url = "https://youtrack.example.com"
token = "perm:base64user.base64name.token"
default_project = "PROJ"
```

#### Jira Configuration

```toml
# .track.toml
backend = "jira"

[jira]
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"
```

#### Linear Configuration

```toml
# .track.toml
backend = "linear"
default_project = "PROJ" # Linear team key/name/id

[linear]
token = "lin_api_xxxxxxxxxxxx"
url = "https://linear.app/your-workspace" # for `track open`
# api_url = "https://api.linear.app/graphql" # default
# default_team = "PROJ" # optional alias for default_project
# default_linear_project = "Track CLI" # optional issue Project association
```

#### Multi-Backend Configuration

You can configure multiple backends in a single config file and switch between them:

```toml
# .track.toml
# Set your default backend
backend = "youtrack"

# YouTrack configuration
url = "https://youtrack.example.com"
token = "perm:base64user.base64name.token"
default_project = "PROJ"

# Jira configuration
[jira]
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"

# Optional: override link type name mappings per backend
# [jira.link_mappings]
# depends = "Requires"
```

With this setup, you can use the default backend (YouTrack) or switch to another backend:

```bash
track PROJ-123                  # Uses YouTrack (default)
track -b jira PROJ-123          # Uses Jira
track -b lin ORE-123            # Uses Linear

# Or switch the default backend
track config backend jira       # Set Jira as default
track config backend linear     # Set Linear as default
```

### Environment Variables

Environment variables override config file settings:

```bash
# Generic (any backend)
export TRACKER_BACKEND=youtrack
export TRACKER_URL=https://youtrack.example.com
export TRACKER_TOKEN=YOUR_TOKEN

# YouTrack-specific
export YOUTRACK_URL=https://youtrack.example.com
export YOUTRACK_TOKEN=YOUR_TOKEN

# Jira-specific
export JIRA_URL=https://your-domain.atlassian.net
export JIRA_EMAIL=you@example.com
export JIRA_TOKEN=your-api-token

# GitHub-specific
export GITHUB_TOKEN=ghp_xxx
export GITHUB_OWNER=your-org
export GITHUB_REPO=your-repo

# GitLab-specific
export GITLAB_TOKEN=glpat_xxx
export GITLAB_URL=https://gitlab.com/api/v4
export GITLAB_PROJECT_ID=12345

# Linear-specific
export LINEAR_TOKEN=lin_api_xxx
export LINEAR_URL=https://linear.app/your-workspace
export LINEAR_DEFAULT_TEAM=PROJ
# export LINEAR_API_URL=https://api.linear.app/graphql
# export LINEAR_DEFAULT_PROJECT="Track CLI"

# HTTPS behind a proxy that inspects it (see "HTTPS proxies and certificates")
export SSL_CERT_FILE=/path/to/proxy-ca.pem
export TRACKER_SYSTEM_CERTS=true
```

## Backend Selection

Default backend is YouTrack. You can specify which backend to use in three ways:

### 1. Config File (Recommended)

```toml
# .track.toml
backend = "youtrack"  # or "jira", "github", "gitlab", "linear"
```

Or use the CLI to set it:

```bash
track config backend youtrack
track config backend jira
track config backend linear
```

### 2. Environment Variable

```bash
export TRACKER_BACKEND=jira
track PROJ-123              # Uses Jira
```

### 3. Per-Command Flag

```bash
track -b jira PROJ-123      # Use Jira for this command
track -b youtrack PROJ-123  # Use YouTrack
track -b linear PROJ-123    # Use Linear

# Short aliases
track -b j PROJ-123         # Jira
track -b yt PROJ-123        # YouTrack
track -b lin PROJ-123       # Linear
```

**Priority**: CLI flag > Environment variable > Config file > Default (YouTrack)

### Repository configs and your token

A `.track.toml` committed to a repository is read whenever you run `track` in
that directory. Because config layers merge key by key, such a file could set
only a URL and have `track` send the token from your global config,
environment or `--token` to a host the repository chose. `track` refuses that
before sending anything:

```
Error: Refusing to send your youtrack token to https://attacker.example.
```

The check only applies when the repository file sets a token destination
(`url`, `[backend].url`, `github.api_url`, `linear.api_url`) that differs from
your own configuration's and does not also supply the token. A repository
`.track.toml` holding both URL and token (what `track init` writes), or only
settings such as `default_project`, is unaffected. To allow a repository you
trust, or to bypass its config:

```bash
track config trust          # Allow this directory's .track.toml to receive your token
track config untrust        # Revoke it (changing the URL also requires trusting again)
track --config ~/.tracker-cli/.track.toml ...   # Ignore the repository's .track.toml
```

`track config show` and `track doctor` report a redirected token as well.

### HTTPS proxies and certificates

`track` trusts the Mozilla root certificates built into it, the same set on
every machine. A corporate proxy that inspects HTTPS re-signs traffic with its
own certificate authority, so requests fail with `invalid peer certificate:
UnknownIssuer`. Trust that authority in either of two ways:

```bash
export SSL_CERT_FILE=/path/to/proxy-ca.pem   # PEM file(s), as OpenSSL-based tools read them
                                             # (SSL_CERT_DIR works too)
track config set system_certs true           # Or trust your operating system's certificates
                                             # (TRACKER_SYSTEM_CERTS=true for one run)
```

Both add certificates to the built-in roots; nothing they trusted before is
dropped. `SSL_CERT_FILE` / `SSL_CERT_DIR` take precedence over the system
store when set. With neither, `track` uses exactly the built-in roots.

## Commands

### Issue Shortcuts

```bash
track PROJ-123              # Get issue (shortcut)
track PROJ-123 --full       # With comments, links, subtasks
track open PROJ-123         # Open in browser
```

### Issue Commands

```bash
# Get
track issue get PROJ-123
track i g PROJ-123 --full

# Create (fields are validated by default)
track issue create -p PROJ -s "Summary" -d "Description"
track i new -s "Subtask" --parent PROJ-100 --priority "Major"
track i new -p PROJ -s "Title" --field "Priority=Major"
track i new -p PROJ -s "Title" --field "Priority=Major" --dry-run   # Check only
track i new -p PROJ -s "Title" --field "Custom=x" --no-validate     # Send as given

# Update (single or batch)
track issue update PROJ-123 --summary "New title"
track i u PROJ-123 --field "Priority=Critical"
track i u PROJ-1,PROJ-2,PROJ-3 --field "Priority=Major"  # Batch update
track i u PROJ-123 --assignee @username     # GitLab: @-prefixed username (case-insensitive)
track i u PROJ-123 --assignee 7            # GitLab: numeric user id (verified server-side)
track i u PROJ-123 --assignee ""            # Clear assignees (GitLab also accepts "0" or "@0")

# State transitions (single or batch)
track issue start PROJ-123              # In progress
track issue complete PROJ-123           # Done
track i start PROJ-1,PROJ-2,PROJ-3      # Batch start
track i done PROJ-1,PROJ-2 --state Done # Batch complete

# Search (with query or template)
track issue search "project: PROJ #Unresolved" --limit 20
track i s "project: PROJ State: Open"
track i s --template unresolved --project PROJ  # Use query template
track i s "project: PROJ #Unresolved" --all     # Fetch all pages
track i s "project: PROJ #Unresolved" -o json --select id_readable,summary  # Projected JSON
track i s "project: PROJ #Unresolved" -o jsonl  # JSON Lines: one issue per line
track i s "project: PROJ #Unresolved" -o jsonl --select id_readable,summary  # Projected JSON Lines

`--select` (JSON/JSONL only) takes comma-separated dotted paths (e.g. `id_readable,project.short_name`)
and emits only those fields per issue; missing paths are omitted. It is rejected in text mode and
for any command other than `issue search`.

# Inspect many issues at once (per-issue success/failure, opt-in context)
track issue inspect PROJ-1,PROJ-2,PROJ-3 -o json
track i ix --ids ids.txt --include comments,links -o json   # IDs from file ("-" for stdin)
track i ix --query "project: PROJ #Unresolved" --all --include all -o json
track i ix --template unresolved --project PROJ --limit 50 --jsonl  # One JSON object per line
track i ix PROJ-1,PROJ-404 --strict     # Exit non-zero if any issue fails (after full report)

# Delete (single or batch)
track issue delete PROJ-123
track i del PROJ-1,PROJ-2,PROJ-3        # Batch delete
```

#### Field validation

Field updates are checked before they are sent, on every `issue create`, `issue update` and
`apply` operation:

| Mode | Flag | What is rejected |
|------|------|------------------|
| Default | (none) | Only what the backend would reject or silently drop (see below) |
| Strict | `--validate` | What the default rejects, plus any field name not in the project schema and any value outside a listed value set. Names and values the backend itself takes (`State` for `Status`, `done` on GitHub) still pass. Failing to fetch the schema is an error |
| Off | `--no-validate` | Nothing; fields are sent as given |

What the default mode can reject depends on how much a backend's schema says:

| Backend | Unknown field name | Invalid value |
|---------|--------------------|---------------|
| YouTrack | Error | Error, for every field with a value list |
| GitHub, GitLab | Error. Only the state and `Assignee` are settable with field flags; use `--tag` for labels | Error for the state (anything that is not open/closed or an alias of them) |
| Jira | Warning | Error for `Priority`, `Type` and `Components` when their lists were read from the instance; the status is left to the API |
| Linear | Error. The state, `Priority`, `Assignee`, `Labels` (or `Tags`) and `Project` are settable | Error for the state and `Priority`; assignee, labels and project are left to the API |

An error names the fix: `Unknown field 'Prio'. Did you mean 'Priority'? Available fields: State,
Priority, Type`, or the list of valid values with the nearest one.

The default check never costs an extra request. It uses the schema the command already needs
(any `--field` needs it for typing) or a cached one. With only `--state`, `--priority` or
`--assignee` and no cache, nothing is checked. `--dry-run` no longer needs `--validate`: it
checks, reports, and writes nothing. A dry run always reads the schema, and asks the tracker for
the issue it would update, so when either cannot be had it fails instead of reporting fields it
did not check.

`issue start` and `issue complete` check the state before they write. When the project's state
field has no state with the backend's default name, the command fails without touching the
issue and lists the states the project does have (`Invalid value 'Develop' for field 'State'.
Valid values: Open, In Progress, Done. Pass --state <NAME> to choose one.`). `issue complete`
without `--state` uses the project's only resolved state when there is exactly one. An explicit
`--field` skips the check and every lookup.

### Comments

```bash
track issue comment PROJ-123 -m "Comment text"
track issue comments PROJ-123 --limit 10
```


### History

Show an issue's change history — the time-ordered timeline of field transitions
(status changes, assignee changes, etc.) with timestamps and authors. Supported
on all backends.

```bash
track issue history PROJ-123                       # Full timeline, newest first
track i hist PROJ-123 --field status               # Only status transitions
track i history PROJ-123 --since 7d                 # Last 7 days (s/m/h/d/w)
track -o json i history PROJ-123                    # {"issue": id, "changes": [...]}
track -b gh i history 42                            # GitHub (numeric id)
```

`from`/`to` coverage varies by backend: Jira, YouTrack, and Linear carry the
prior value for every field; the event-based backends (GitHub, GitLab) populate
`from` only for `status` and report `from: null` for other fields. (Linear
label-change history is not yet included.)


### Links

```bash
track issue link PROJ-1 PROJ-2              # Relates (default)
track issue link PROJ-1 PROJ-2 -t depends   # Depends on
track issue link PROJ-1 PROJ-2 -t subtask   # Subtask
track issue link PROJ-1 PROJ-2 -t clones    # Custom/admin-defined type

# Unlink (remove a link by its ID — get link IDs from `track i g PROJ-1 --full`)
track issue unlink PROJ-1 "142-3t/PROJ-2"   # YouTrack (composite ID)
track -b j issue unlink PROJ-1 12345         # Jira (numeric link ID)
track -b gl issue unlink 42 789              # GitLab (numeric link ID)
track -b lin issue unlink ORE-1 <link-id>    # Linear (relation ID or linear-parent/child ID)
```

**Built-in link types**: `relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, `parent`

Unrecognized type names are passed through to the backend as-is, so admin-defined link types (e.g., `clones`, `causes`) work without CLI changes.

#### Custom Link Type Mappings

Each backend maps canonical link type names (like `depends`) to its native name (e.g., `"Blocks"` on Jira, `"Depend"` on YouTrack). You can override these mappings in your config file:

```toml
# Override Jira's default mapping for "depends" (default: "Blocks")
[jira.link_mappings]
depends = "Requires"
duplicates = "Cloners"

# Override YouTrack's default mapping
[youtrack.link_mappings]
depends = "Is required for"

# Override GitLab's default mapping
[gitlab.link_mappings]
depends = "is_blocked_by"

# Override Linear's default mapping
[linear.link_mappings]
relates = "similar"
```

This is useful when your instance has custom or renamed link types.


### Projects

```bash
track project list
track project get PROJ
track project fields PROJ       # Custom fields
track project create -n "Name" -s "KEY"  # YouTrack only (defaults leader to authenticated user; --leader is ignored for non-YouTrack backends)
track project create -n "Name" -s "KEY" -l "username"  # Specify explicit leader (YouTrack only)
```

### Tags

```bash
track tags list  # Lists tags/labels for the configured backend
```

**Note**: GitHub and GitLab use labels instead of tags. The CLI maps labels to the common `IssueTag` model.

### Custom Fields Admin (YouTrack only)

```bash
# List all custom field definitions
track field list

# Create a field definition
track field create "Priority" -t enum

# Create field with values and attach to project (convenience command)
track field new "Sprint Phase" -t enum -p PROJ -v "Planning,Development,Testing,Done"

# For state fields with resolved markers
track field new "Bug Status" -t state -p PROJ -v "Open,In Progress,Fixed,Closed" --resolved "Fixed,Closed"

# List bundles by type
track bundle list -t enum
track bundle list -t state

# Create a bundle with values
track bundle create "Priority Levels" -t enum -v "Low,Medium,High,Critical"

# Add value to existing bundle
track bundle add-value <bundle-id> -t enum -v "Urgent"

# Attach a field to a project
track project attach-field PROJ -f <field-id> --bundle <bundle-id>
```

### Articles (Knowledge Base)

YouTrack uses its built-in Knowledge Base. Jira uses Confluence (automatically at same domain with `/wiki` path).

```bash
# YouTrack
track article get KB-A-1
track article list --project PROJ
track article search "query"
track article create --project PROJ --summary "Title" --content "Body"
track article update KB-A-1 --content-file ./doc.md

# Jira/Confluence (use numeric space ID for --project)
track -b j article get 123456
track -b j article list --project 65957 --limit 20
track -b j article search "query"
track -b j article create --project 65957 --summary "Title" --content "Body"
track -b j article update 123456 --summary "New Title"
track -b j article delete 123456
```

### Config

```bash
track config test           # Test connection
track config show           # Show current config
track config backend jira   # Set default backend
track config project PROJ   # Set default project
track config set <key> <value>  # Set any config value
track config get <key>      # Get a config value
track config keys           # List all available config keys
track config clear          # Clear default project and backend
track config path           # Show config file path
track config trust          # Allow this directory's .track.toml to receive your own token
track config untrust        # Revoke config trust for this directory
track config set system_certs true  # Also trust the OS certificate store (HTTPS proxies)
```

### Doctor (Backend Capability Audit)

```bash
track doctor                       # Audit the configured backend
track doctor --all-backends        # Audit every configured backend
track -b gitlab doctor             # Audit a specific backend
track doctor --project PROJ       # Use a specific project for scoped checks
track doctor --write-check         # Also validate write payloads locally (no remote writes)
track doctor --all-backends --strict -o json  # CI-friendly: non-zero exit if any check or backend failed
```

**`config test` vs `doctor`:** `config test` runs a single connectivity probe
(`list_projects`) and tells you whether the URL and token work at all.
`doctor` audits what the backend can *actually do*: it runs non-mutating checks
for config validity, auth/connectivity, project resolution, issue search/read,
comments, links, field schema, field admin, and articles, and reports each as
`ok`, `degraded` (e.g. valid token missing a scope), `failed`, or `skipped`
(capability not supported by the backend). A token can pass `config test` but
lack scopes for specific operations — or fail `config test` while search/read
still work; `doctor` distinguishes those cases and never mutates remote
trackers (`--write-check` only validates against the locally fetched field
schema). A backend rolls up `failed` only when nothing practical works — bad
credentials, or a broken read path (e.g. every call 404s under a wrong
project id). Exit code is 0 unless `--strict` is passed and a check or
backend `failed` (degraded stays 0).

### Cache

```bash
track cache refresh              # Refresh local cache (includes issue counts)
track cache refresh --if-stale 1h  # Only refresh if older than 1 hour
track cache status               # Check cache age and freshness
track cache show                 # Show cached data
track cache path                 # Show cache location
```

The cache stores comprehensive tracker context for fast lookups:
- Projects, custom fields (with enum values), and tags
- Issue link types and workflow hints (state transitions)
- Assignable users per project
- Knowledge base articles
- Query templates for both backends
- Issue counts per project and query template
- Recently accessed issues (LRU, max 50)

**Read-through lookups.** Commands that need a project's id or schema to build a write take
them from the cache when it holds an answer no older than 24 hours, and write back what they
had to fetch. After one `track cache refresh`, a validated `issue create` is a single request,
and so is `issue start`. A cached "no" is never final: before an unknown field or invalid value
becomes an error, the schema is fetched once more from the tracker, and a cached project id the
tracker turns down (the project was deleted and recreated) is looked up again before the write
fails. Commands that exist to read (`project fields`, `project list`, `tags list`, `issue get`)
always ask the tracker for what they show, and `project fields` and `tags list` refresh the cache
with what they read.

| Control | Effect |
|---------|--------|
| `--no-cache` or `TRACK_NO_CACHE=1` | Answer every lookup from the tracker (`0`, `false`, `off` or empty leave the cache on). Changes a command makes, such as a new tag or project, still reach the cache |
| `TRACK_SCHEMA_TTL=7d` | How long a cached schema or project id is trusted (default `24h`) |
| `TRACK_CACHE_DIR=<path>` | Use this directory as the cache, instead of the project or global one |

**One directory per backend.** The cache lives in `.tracker-cache/<backend>/` (or
`~/.tracker-cli/cache/<backend>/`), so switching backends in one directory no longer makes each
refresh overwrite the other. A cache in the old single-directory layout is moved into place the
first time it is used. Lookups, `track prime`, `track context` and `cache refresh --if-stale`
only trust a cache written for the same instance (API endpoint, plus the repository or project
for GitHub and GitLab): `prime` says to refresh any other, `context` fetches afresh, and
`--if-stale` refreshes it however recent it is.

Built-in templates are fitted to each project whose schema is cached (YouTrack and Jira): they
use the project's own state field name, and `high_priority`, `in_progress` and `bugs` are built
from the values the project really has. A template with no fitting value is left out for that
project, and `-T` says so instead of running a query that matches nothing.

Cached built-in templates are backend-scoped: the cache records which backend wrote it, and
`workflow list` / template resolution only expose templates whose backend matches the selected
one. A repository workflow pack (`[workflow_pack]` in `.track.toml`) takes precedence over
cache-built-ins on name collision (shadowing); pack queries are never written into the cache.
Each backend keeps its own cache directory, so run `track cache refresh` once per backend.

### Workflow Packs

A `[workflow_pack]` in `.track.toml` defines project-local query templates:

```toml
[workflow_pack]
name = "Orek backlog"
default_project = "PROJ"

[[workflow_pack.queries]]
name = "ready"
description = "Issues ready for implementation."
query = "project: {PROJECT} #Unresolved State: {Ready}"
```

```bash
track workflow list      # Pack queries first, then unshadowed cached built-ins (offline)
track workflow show      # Selected pack, source, and queries
track workflow validate  # Offline; shadows_builtin is a warning, semantic errors fail
track i s -T ready -p PROJ
```

- One pack is selected whole: `--config`/`TRACKER_CONFIG`, else `./.track.toml`, else
  `~/.tracker-cli/.track.toml`. Project config shadows the global config.
- Query names must be lowercase snake_case (`^[a-z][a-z0-9_]*$`), unique within the pack
  (case-insensitive), with non-empty name/description/query. Only exact `{PROJECT}` is
  substituted (`--project`, then pack `default_project`, then config `default_project`).
- A pack query shadows the same-named built-in (`validate` warns `shadows_builtin`); pack
  queries always win over cached built-ins. Packs are never written into the cache.
- An invalid pack fails pack-consuming commands (`-T` search, `workflow` commands) instead
  of silently falling back to built-ins; ordinary commands like `issue get` are unaffected.

### Prime (agent briefing)

```bash
track prime                # Compact briefing: states, required fields, values, users, templates
track prime -p PROJ        # Brief on another cached project
track prime --max-values 5 # Tighter cap per field (default 12)
track -o json prime        # The same facts as JSON
track prime --hook=claude  # For session hooks (claude, codex, cursor, antigravity, gemini, orek)
```

`track prime` prints what an agent needs before it touches this project: the real state names
(with the resolved ones marked), the fields required on create, every enum value, assignable
users, tags, the `-T` templates with counts, and a few commands in the backend's own query
syntax. It reads only the local cache and config, never the network, and stays within a few
hundred tokens. Long lists are cut to a cap and say how many entries were left out and which
command lists them. Run `track cache refresh` once to populate the cache; a cache older than a
day, or one written for another instance (a changed URL, a committed `.tracker-cache/`), gets a
warning line instead of being presented as this tracker's.

`--hook` is for running it automatically at the start of an agent session: it prints nothing
and exits 0 unless the current directory has its own `.track.toml` (or `--config` is given),
and it never fails.

### Context (AI-Optimized)

```bash
track context                    # Aggregated context for AI sessions
track context --project PROJ     # Filter to specific project
track context --refresh          # Force refresh from API
track context --include-issues   # Include unresolved issues
track -o json context            # JSON output for parsing
```

Single command to get all relevant data: projects, fields, users, query templates, workflow hints, issue counts, and recent issues.

The JSON also says how old that data is: `cache_updated_at`, `cache_age_seconds` and `stale`
(true after 24 hours). `generated_at` is only when the command ran. `current_user` is the
account the token belongs to, when the backend reports it.

## Command Aliases

| Full Command | Aliases |
|--------------|---------|
| `track issue` | `track i` |
| `track issue get` | `track i g` |
| `track issue create` | `track i new`, `track i c` |
| `track issue update` | `track i u` |
| `track issue search` | `track i s`, `track i find` |
| `track issue inspect` | `track i ix` |
| `track issue delete` | `track i rm`, `track i del` |
| `track issue comment` | `track i cmt` |
| `track issue history` | `track i history`, `track i hist` |
| `track issue complete` | `track i done`, `track i resolve` |
| `track issue start` | `track i start` |
| `track issue link` | `track i link` |
| `track issue unlink` | `track i ul` |
| `track project` | `track p` |
| `track project list` | `track p ls` |
| `track project fields` | `track p f` |
| `track tags` | `track t` |
| `track article` | `track a`, `track wiki` |
| `track config` | `track cfg` |
| `track context` | `track ctx` |
| `track field list` | `track field ls` |
| `track field create` | `track field c` |
| `track field new` | `track field setup` |
| `track bundle list` | `track bundle ls` |
| `track bundle create` | `track bundle c` |

## Query Syntax by Backend

### YouTrack

```bash
track i s "project: PROJ #Unresolved"
track i s "project: PROJ State: {In Progress}"
track i s "project: PROJ Assignee: me Priority: Major"
```

### Jira (JQL)

```bash
track -b jira i s "project = PROJ AND resolution IS EMPTY"
track -b jira i s "project = PROJ AND status = 'In Progress'"
track -b jira i s "assignee = currentUser() AND priority = Major"
```

### GitHub

```bash
track -b github i s "is:open label:bug"
track -b github i s "is:closed assignee:username"
track -b github i s "is:issue is:open"
```

GitHub uses GitHub's search query syntax (not traditional issue queries).

### GitLab

```bash
track -b gitlab i s "bug fix" --state opened
track -b gitlab i s "performance" --labels "priority::high"
```

GitLab uses project-scoped search with filter parameters.

### Linear

```bash
track -b linear i s "project: ORE #Unresolved"
track -b linear i s "team: ORE state: {In Progress}"
track -b linear i s "project: ORE label: Bug assignee: me"
```

Linear exposes teams as `track` projects. The `Project` field on issues maps to Linear's native project association (`--field "Project=Track CLI"`).

## Output Formats

```bash
track PROJ-123              # Text (default)
track -o json PROJ-123      # JSON
track --format json p ls    # JSON
track -o jsonl p ls         # JSON Lines: one project per line
track -o agent PROJ-123     # Compact text for AI agents
```

`-o jsonl` prints the same document as `-o json`, compacted for line-based tools: when the
document is an array (a list command such as `p ls` or `i s`), each element is one line, and an
empty list prints nothing; anything else (an object such as `track PROJ-123` or a batch summary)
is a single line. Nested arrays stay inside their line. Every command that prints JSON accepts
it; `completions` rejects it. In both modes stdout holds only JSON; warnings, progress and
errors go to stderr.

### Agent output

`-o agent` is text written for an AI agent to read: no colour, no decoration, and nothing cut
without saying so. Set `TRACK_OUTPUT=agent` to make it the default (an explicit `-o` still
wins; an empty or unknown value is ignored).

```
$ track -o agent i s "project: DEMO #Unresolved" --limit 2
# id	state	priority	assignee	summary
DEMO-1	Open	Normal	jdoe	Implement user authentication
DEMO-2	In Progress	High	-	Fix login timeout
# 2 shown of 57; more may exist: --skip 2, or --all for everything
```

| Command | Agent output |
|---------|--------------|
| `issue search` | One tab-separated line per issue, then a last line that says `complete` or how to get the rest |
| `issue get` | Summary, every set field on one line, then the full description |
| `issue get --full` | The same, plus every link (with its link id) and the last 20 comments in full; more are named with the command that lists them |
| `issue create` / `update` | One line: what was done and the field values the issue ended up with |
| `issue comments`, `project fields`, batch results | One line per item; `project fields` includes the values |
| `article list` / `search` | One line per article, without its content (`article get` prints it whole), then the same last line as `issue search` |
| `context` | The `track prime` briefing |
| everything else | The usual text, uncoloured |

It is typically a third to a fifth of the size of the JSON for the same data.

### Selecting fields

`--select` keeps only the listed fields of whatever JSON a command prints: comma-separated
dotted paths, on any command, with `-o json` or `-o jsonl`. Paths walk into nested arrays, so
they work on the `--full` and `inspect` envelopes too.

```bash
track -o json i s "project: PROJ #Unresolved" --select id_readable,summary,fields.Priority
track -o json PROJ-1 --full --select issue.id_readable,issue.fields.Stage,comments.text
track -o jsonl p ls --select short_name
```

- `fields.<Name>` is an issue's custom field by name, as a plain value
  (`{"fields": {"Priority": "Major"}}`); a field the issue has but with no value is `null`. It
  exists only under `--select`; the default JSON is unchanged. Quote a name that holds `.` or
  `,`: `fields."Est. time"`.
- A path missing from one record is omitted from that record.
- A selection that matches nothing at all is an error (`select_no_match`, exit 5), not `{}`.
  After a write (create, update, delete, comment, ...) it is a warning on stderr instead: the
  write is done, so the command prints its whole JSON output and exits 0.
- Text output (including `-o agent`) rejects the flag.

### Pagination and write results

When a search or list fills its limit, a one-line hint with the next `--skip` value (and a
total when one is known) goes to stderr, in JSON and JSONL mode too: a full page of JSON
otherwise looks exactly like a complete result. `-o agent` puts the verdict on the last line of
stdout instead, `complete` or how to get the rest, for `issue search`, `article list` and
`article search` alike.

`issue start` / `issue complete` JSON reports what the tracker did, not only what was asked:
`state` is the requested state, `actual_state` the one the issue is in afterwards, `verified`
whether they agree, and `warnings` why not. `id_readable` is the issue id.

Subcommand help lists that command's own options. The global options (`-o`, `--select`, `-v`,
`--color`, `-b`, `--config`, `--url`, `--token`, `--no-cache`, `--error-format`) are documented
once, in `track --help`, and accepted everywhere.

## Errors

A failure prints up to three lines on stderr and exits with a code that names its kind:

```
Error: Unknown field 'Prio'. Did you mean 'Priority'? Available fields: State, Priority, Type
Code: unknown_field
Hint: `track prime` lists this project's fields and their values; --no-validate sends the fields as given.
```

`Code:` is a short, stable name to branch on; `Hint:` is the next step, when one is known.
With `-o json` or `-o jsonl` the error is one JSON object on one line of stderr instead:
`{"error": {"code", "message", "hint", "retryable"}}`. `--error-format text|json` (or
`TRACK_ERROR_FORMAT`) chooses explicitly. `retryable` is true when the same command may work
if run again later.

The exit code says what kind of failure it was:

| Exit | Meaning | Codes |
|------|---------|-------|
| 0 | Success (a write the tracker only partly applied still exits 0, with a warning on stderr) | |
| 1 | Any other failure | `api`, `parse`, `io`, `pagination_stalled`, `error` |
| 2 | The command line did not parse | (clap's own message) |
| 3 | Not found | `issue_not_found`, `project_not_found`, `not_found`, `template_not_found`, `unknown_topic` |
| 4 | Not allowed or not set up | `auth`, `permission`, `not_configured`, `token_refused` |
| 5 | Refused as given: fix the command | `unknown_field`, `invalid_value`, `read_only_field`, `invalid_state`, `invalid_input`, `select_no_match`, `unsupported`, `conflict` |
| 6 | Some items of a batch failed | `partial_failure` |
| 7 | Retryable: run it again later | `network`, `rate_limited`, `server` |


| Code | Meaning |
|------|---------|
| `auth` | The token was rejected |
| `permission` | The token is valid but may not do this |
| `issue_not_found`, `project_not_found`, `not_found` | No such issue, project or other resource (or the token cannot see it). `project_not_found` names the closest cached project key |
| `unknown_field`, `invalid_value`, `read_only_field` | Field validation failed; the message lists what is valid |
| `invalid_state` | `issue start` / `complete` asked for a state the project does not have |
| `template_not_found` | No such `-T` template, or it does not apply to this project |
| `unknown_topic` | `track guide` has no such topic; the message lists the topics |
| `select_no_match` | `--select` paths are not in this command's JSON (a read; after a write it is a warning) |
| `not_configured`, `token_refused` | No usable config, or a repository config tried to redirect the token |
| `invalid_input`, `unsupported`, `conflict`, `api` | The tracker refused the request |
| `rate_limited`, `server`, `network` | Retryable: the tracker is limiting, failing or unreachable |
| `parse`, `pagination_stalled`, `io` | An unexpected response or a local file problem |
| `error` | Anything else |

Per-issue failures in batch results (`issue update A,B`, `issue inspect`) carry the same
`code` beside their `error` text.

## Upgrading to 2.0

2.0 changes four contracts. Everything else added since 1.19 (`track prime`, `-o agent`,
default validation, `track agent install`, error codes) arrived without breaking anything.

| Change | Before | Now | If you depended on it |
|--------|--------|-----|-----------------------|
| Exit codes | `1` for every failure | `1`-`7` by kind (see [Errors](#errors)) | Test for non-zero, or for the specific code |
| Errors in JSON mode | Plain text on stderr | One JSON object on stderr | Pass `--error-format text`, or set `TRACK_ERROR_FORMAT=text` |
| `workflow_hints[].state_fields[].transitions` in `context` and `cache show` JSON | A list of every state pair | Removed; `states` (in order, with `is_resolved`) remains | The list was computed from `states` and added no information |
| `idReadable` in `issue start` / `complete` JSON | Present | Removed | Read `id_readable`, as in every other command |

Changed earlier in the 1.x line and worth knowing when coming from 1.19:

- Field validation is on by default (1.21). `--no-validate` restores the old behaviour.
- `issue start` / `complete` fail before writing when the default state does not exist in the
  project, and list the states that do (1.21).
- The cache lives in `.tracker-cache/<backend>/`; an old cache is moved there on first use (1.21).
- A full page of search results prints its pagination hint to stderr in JSON mode too (1.22).

## Backend-Specific Notes

### YouTrack
- Full feature support including custom fields, field admin, and knowledge base
- Uses Bearer token authentication
- Rich query language for issue search

### Jira
- **Knowledge Base**: Uses Confluence API (automatically at same domain with `/wiki` path)
- **Authentication**: Basic Auth with email and API token
- **Rich Text**: Uses Atlassian Document Format (ADF) for descriptions. ADF rich-text *custom* fields (e.g. "Repro Steps", "Expected Results") are surfaced as rendered plain text, the same way descriptions and comments are.
- **Project Creation**: Requires admin permissions (use web interface)
- **Subtasks**: Create as subtask with `--parent`, or link existing issues with `issue link -t subtask`
- **Labels**: Map to tags
- **System & custom fields**: `issue get`/`issue search` surface *all* populated fields Jira returns — standard system fields (`fixVersions`, `reporter`, `environment`, `duedate`, `resolution`, …) and every custom field — as `custom_fields` entries, not just a hardcoded subset. Single-issue reads (`track <KEY>`) fetch the full field set too. Anything that can't be mapped to a typed variant is preserved verbatim as `Unknown { value }`, so no data is lost. `--field <name>` write attempts flow straight to Jira; if a field isn't editable, Jira's error is surfaced.
- **Components**: Jira's standard Components field is surfaced as a `Components` multi-value custom field. To filter by area, use server-side JQL such as `component = "Rendering"`.

### GitHub
- **Scope**: Repository-scoped (requires owner and repo configuration)
- **Issue IDs**: Uses numeric issue numbers (e.g., `42`), not project-prefixed keys
- **Labels**: Map to tags with color support
- **No Issue Deletion**: GitHub does not support deleting issues (close them instead)
- **Subtasks**: Supported via the sub-issues API (`--parent`, `issue link -t subtask/parent`)
- **No General Issue Links**: GitHub has no formal link system for non-parent-child relationships (reference issues via `#number` in comments)
- **Pull Requests**: Automatically filtered out from issue lists
- **Rate Limiting**: May encounter rate limits on public API; use authenticated requests

### GitLab
- **Scope**: Project-scoped via `project_id` configuration
- **Issue IDs**: Uses IID (project-scoped, e.g., `#42`), not global IDs
- **Labels**: Map to tags with color support (includes `#` prefix)
- **Comments**: Called "notes" in GitLab API; system notes are filtered out
- **API Version**: Uses GitLab REST API v4 (with GraphQL for parent-child)
- **Subtasks**: Supported via the GraphQL API (`--parent`, `issue link -t subtask/parent`)

### Linear
- **Scope**: Team-scoped for CLI projects (`-p ORE` maps to a Linear team)
- **API**: Uses Linear GraphQL with personal API keys (`Authorization: <API_KEY>`)
- **Projects**: Linear projects are issue associations, set with `--field "Project=Track CLI"` or `linear.default_linear_project`
- **Labels**: Map to tags; unknown labels on create/update are rejected
- **Subtasks and Links**: Parent-child uses `parentId`; relation links support `related`, `blocks`, `duplicate`, and `similar`
- **Knowledge Base**: Article commands are not supported

## Architecture

```
crates/
├── tracker-core/       # Core traits and models
├── youtrack-backend/   # YouTrack API client
├── jira-backend/       # Jira API client
├── github-backend/     # GitHub API client
├── gitlab-backend/     # GitLab API client
├── linear-backend/     # Linear GraphQL client
├── tracker-mock/       # Mock system for testing
├── agent-harness/      # AI agent evaluation harness
└── track/              # CLI binary
```

- **tracker-core**: `IssueTracker` trait, common models, errors
- **youtrack-backend**: YouTrack REST API with Bearer auth
- **jira-backend**: Jira Cloud REST API v3 with Basic Auth
- **github-backend**: GitHub REST API with token auth
- **gitlab-backend**: GitLab REST API v4 with Private-Token auth
- **linear-backend**: Linear GraphQL API with personal API key auth
- **tracker-mock**: Mock backend for testing and evaluation
- **agent-harness**: AI agent testing and evaluation tool
- **track**: CLI with clap, figment config, text/JSON output

## Development

```bash
# Build
cargo build

# Test (unit + mock integration tests)
cargo test

# Test specific crate
cargo test --package youtrack-backend
cargo test --package jira-backend
cargo test --package github-backend
cargo test --package gitlab-backend
cargo test --package linear-backend

# Run live integration tests (requires .track.toml with valid credentials)
cargo test --package track --test youtrack_integration_tests -- --ignored
cargo test --package track --test jira_integration_tests -- --ignored
cargo test --package track --test github_integration_tests -- --ignored
cargo test --package track --test gitlab_integration_tests -- --ignored
cargo test --package track --test linear_integration_tests -- --ignored

# Run without installing
cargo run -- PROJ-123
```

## Adding a Backend

1. Create `crates/<backend>-backend/`
2. Implement `IssueTracker` trait from `tracker-core`
3. Add model conversions to/from common `tracker-core` types
4. Register in `crates/track/src/main.rs` backend selection
5. Add config support in `crates/track/src/config.rs`
6. Add unit tests with `wiremock` for HTTP mocking
7. Update documentation

See existing backend crates (`jira-backend/`, `github-backend/`, `gitlab-backend/`) for reference implementations.

## For AI Agents

### Setup

```bash
track agent install                  # For you: every supported tool, in your home directory
track agent install --scope project  # For the team: writes into this repository (commit the result)
track agent install --tools orek,claude   # Only the tools you name
track agent status                   # What is installed, per tool, for you and for this directory
track agent uninstall [--scope project] [--tools ...]
```

Each tool gets what its own documentation says it reads, and nothing it does not:

| Tool | Skill (user / project) | Session-start hook (user / project) | Instructions |
|------|------------------------|-------------------------------------|--------------|
| Orek.ai | Orek's user config `skills/` / `.orek/skills/` | none yet: the hook is ready for when Orek.ai ships session hooks | `AGENTS.md` (project) |
| Claude Code | `~/.claude/skills/` / `.claude/skills/` | `~/.claude/settings.json` / `.claude/settings.json` | `~/.claude/CLAUDE.md`; `@AGENTS.md` import in an existing `CLAUDE.md` |
| Codex | `~/.agents/skills/` / `.agents/skills/` | `$CODEX_HOME/hooks.json` / `.codex/hooks.json` | `$CODEX_HOME/AGENTS.md`; `AGENTS.md` |
| Cursor | `~/.agents/skills/` / `.agents/skills/` | `~/.cursor/hooks.json` / `.cursor/hooks.json` | `AGENTS.md` (project) |
| Antigravity | `~/.gemini/config/skills/` / `.agents/skills/` | `~/.gemini/config/hooks.json` / `.agents/hooks.json` | `~/.gemini/GEMINI.md`; `AGENTS.md` |
| Gemini CLI | `~/.agents/skills/` / `.agents/skills/` | `~/.gemini/settings.json` / `.gemini/settings.json` | `~/.gemini/GEMINI.md`; `@./AGENTS.md` import in an existing `GEMINI.md` |
| Copilot | `~/.copilot/skills/` (user only) | not installed | not installed |

Orek's user config folder is `$OREK_HOME/config/` when that is set, else
`~/Library/Application Support/orek-ai/` (macOS), `$XDG_CONFIG_HOME/orek-ai/` or
`~/.config/orek-ai/` (Linux), or `%APPDATA%\orek-ai\config\` (Windows).

- **The session-start hook** runs `track prime --hook=<tool>` when a session starts, and again
  after it is cleared or compacted where the tool supports that. In a directory with its own
  `.track.toml` it hands the project briefing to the agent in that tool's hook format; anywhere
  else it adds nothing. It reads the session's folder from the hook input, so it briefs the
  project the agent opened.
- **Agent output.** The Claude Code hook sets `TRACK_OUTPUT=agent` for the session's shell
  commands, Cursor's hook returns it, and Codex gets it in `config.toml`
  (`[shell_environment_policy] set`). Antigravity, Gemini CLI and Orek.ai have no setting for
  it; the instructions tell the agent to add `-o agent`.
- **Codex** runs a hook only after you trust it: open `/hooks` in Codex once.
- **Cursor** also runs Claude Code's hooks. When both are installed, the Claude Code one stays
  quiet inside Cursor so the briefing arrives once.
- **The skill** is a core of under 6 KB (commands, output modes, error handling, the traps) and
  one reference per topic that an agent reads only when it needs it. Tools that share a folder
  share one copy. `track guide <topic>` prints the same references from the binary.
- **The instructions block** tells the agent to run `track prime` before issue work. It is
  static, so it never goes stale; the user-level one applies only in projects with a
  `.track.toml`. A `CLAUDE.md` or `GEMINI.md` that already exists gets an import of `AGENTS.md`
  rather than a second copy.
- Files are edited in place: key order, permissions and everything already in them are kept,
  and a symlinked settings file stays a link to the file that is edited. A file that is not plain
  JSON (Gemini CLI allows comments) is left untouched and reported under its tool; everything
  else is still installed, and the command exits 6.
- `uninstall` removes only what `install` added (and any file or folder it created, once empty).
  With `--tools`, a skill folder or instructions block another tool still reads (`AGENTS.md`,
  `GEMINI.md`, `~/.agents/skills`) is kept, and the output says which tools read it.
- `--skills-only` and `--hooks-only` install one half. `track init --skills` still works and
  installs the skills for you.

### Reference

See [Agent Guide](docs/agent_guide.md) for:
- AI-optimized features: context command, query templates, workflow hints
- Batch operations for efficient multi-issue updates
- Field validation to prevent API errors
- Query syntax comparison (YouTrack vs Jira JQL)
- Session startup checklist
- JSON output parsing examples

## License

MIT
