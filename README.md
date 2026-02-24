# Track CLI

A command-line interface for issue tracking systems, built with Rust. Supports **YouTrack**, **Jira**, **GitHub**, and **GitLab** with a unified command interface.

## Features

- **Multi-Backend**: YouTrack, Jira, GitHub, and GitLab with the same commands
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

### From Source

```bash
cargo install --path crates/track
```

### Build Release

```bash
cargo build --release
# Binary: target/release/track
```

## Quick Start

### 1. Initialize Configuration

Create a `.track.toml` file in your project directory or `~/.config/track/config.toml` for global configuration:

```bash
# Initialize with YouTrack (default)
track init --url https://youtrack.example.com --token YOUR_TOKEN

# Or initialize with Jira
track init --url https://your-domain.atlassian.net --token YOUR_TOKEN --backend jira --email you@example.com
```

### 2. Set Default Project (Optional)

```bash
track config project PROJ
```

### 3. Test Connection

```bash
track config test
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

#### Multi-Backend Configuration

You can configure both backends in a single config file and switch between them:

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
```

With this setup, you can use the default backend (YouTrack) or switch to Jira:

```bash
track PROJ-123                  # Uses YouTrack (default)
track -b jira PROJ-123          # Uses Jira

# Or switch the default backend
track config backend jira       # Set Jira as default
track PROJ-123                  # Now uses Jira by default
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
```

## Backend Selection

Default backend is YouTrack. You can specify which backend to use in three ways:

### 1. Config File (Recommended)

```toml
# .track.toml
backend = "youtrack"  # or "jira"
```

Or use the CLI to set it:

```bash
track config backend youtrack
track config backend jira
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

# Short aliases
track -b j PROJ-123         # Jira
track -b yt PROJ-123        # YouTrack
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

# Delete (single or batch)
track issue delete PROJ-123
track i del PROJ-1,PROJ-2,PROJ-3        # Batch delete
```

### Comments

```bash
track issue comment PROJ-123 -m "Comment text"
track issue comments PROJ-123 --limit 10
```


### Links

```bash
track issue link PROJ-1 PROJ-2              # Relates (default)
track issue link PROJ-1 PROJ-2 -t depends   # Depends on
track issue link PROJ-1 PROJ-2 -t subtask   # Subtask
```


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
| `track issue delete` | `track i rm`, `track i del` |
| `track issue comment` | `track i cmt` |
| `track issue complete` | `track i done`, `track i resolve` |
| `track issue start` | `track i start` |
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
- **Rich Text**: Uses Atlassian Document Format (ADF) for descriptions
- **Project Creation**: Requires admin permissions (use web interface)
- **Subtask Conversion**: Create as subtask from start with `--parent`

### GitHub
- **Scope**: Repository-scoped (requires owner and repo configuration)
- **Issue IDs**: Uses numeric issue numbers (e.g., `42`), not project-prefixed keys
- **Labels**: Map to tags with color support
- **No Issue Deletion**: GitHub does not support deleting issues (close them instead)
- **No Issue Links**: GitHub has no formal issue link system (reference issues via `#number` in comments)
- **Pull Requests**: Automatically filtered out from issue lists
- **Rate Limiting**: May encounter rate limits on public API; use authenticated requests

### GitLab
- **Scope**: Project-scoped via `project_id` configuration
- **Issue IDs**: Uses IID (project-scoped, e.g., `#42`), not global IDs
- **Labels**: Map to tags with color support (includes `#` prefix)
- **Comments**: Called "notes" in GitLab API; system notes are filtered out
- **API Version**: Uses GitLab REST API v4
- **No Subtasks**: Use issue links instead of native subtask relationships

## Architecture

```
crates/
├── tracker-core/       # Core traits and models
├── youtrack-backend/   # YouTrack API client
├── jira-backend/       # Jira API client
├── github-backend/     # GitHub API client
├── gitlab-backend/     # GitLab API client
├── tracker-mock/       # Mock system for testing
├── agent-harness/      # AI agent evaluation harness
└── track/              # CLI binary
```

- **tracker-core**: `IssueTracker` trait, common models, errors
- **youtrack-backend**: YouTrack REST API with Bearer auth
- **jira-backend**: Jira Cloud REST API v3 with Basic Auth
- **github-backend**: GitHub REST API with token auth
- **gitlab-backend**: GitLab REST API v4 with Private-Token auth
- **tracker-mock**: Mock backend for testing and evaluation
- **agent-harness**: AI agent testing and evaluation tool
- **track**: CLI with clap, figment config, text/JSON output

## Development

```bash
# Build
cargo build

# Test
cargo test

# Test specific crate
cargo test --package youtrack-backend
cargo test --package jira-backend
cargo test --package github-backend
cargo test --package gitlab-backend

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
