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
- **Output Formats**: Text (human-readable) and JSON (machine-readable)
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

Agent skills are optional and installed explicitly after the CLI is available.
`track init --skills` installs the bundled `track` reference for Claude Code,
GitHub Copilot, Cursor, and Gemini CLI:

```bash
track init --skills
```

### Homebrew (Package Manager)

```bash
brew tap OrekGames/tap
brew install track
```

Shell completions for bash, zsh, and fish are installed automatically.

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

```bash
# Initialize with YouTrack (default)
track init --url https://youtrack.example.com --token YOUR_TOKEN

# Or initialize with Jira
track init --url https://your-domain.atlassian.net --token YOUR_TOKEN --backend jira --email you@example.com

# Or initialize with Linear (URL is the Linear workspace URL used by `track open`)
track init --url https://linear.app/your-workspace --token YOUR_LINEAR_API_KEY --backend linear --project PROJ
```

For AI-assisted workflows, install the agent skill reference once per user:

```bash
track init --skills
```

This installs guidance for Claude Code, GitHub Copilot, Cursor, and Gemini CLI.

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

# Create (with validation)
track issue create -p PROJ -s "Summary" -d "Description"
track i new -s "Subtask" --parent PROJ-100 --priority "Major"
track i new -p PROJ -s "Title" --field "Priority=Major" --validate

# Update (single or batch)
track issue update PROJ-123 --summary "New title"
track i u PROJ-123 --field "Priority=Critical"
track i u PROJ-1,PROJ-2,PROJ-3 --field "Priority=Major"  # Batch update

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
track project create -n "Name" -s "KEY"  # YouTrack only
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

### Context (AI-Optimized)

```bash
track context                    # Aggregated context for AI sessions
track context --project PROJ     # Filter to specific project
track context --refresh          # Force refresh from API
track context --include-issues   # Include unresolved issues
track -o json context            # JSON output for parsing
```

Single command to get all relevant data: projects, fields, users, query templates, workflow hints, issue counts, and recent issues.

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
```

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

See [Agent Guide](docs/agent_guide.md) for:
- AI-optimized features: context command, query templates, workflow hints
- Batch operations for efficient multi-issue updates
- Field validation to prevent API errors
- Query syntax comparison (YouTrack vs Jira JQL)
- Session startup checklist
- JSON output parsing examples

## License

MIT
