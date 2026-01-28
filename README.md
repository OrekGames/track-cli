# Track CLI

A command-line interface for issue tracking systems, built with Rust. Supports **YouTrack** and **Jira** with a unified command interface.

## Features

- **Multi-Backend**: YouTrack and Jira with the same commands
- **Issue Management**: Get, create, update, delete, search issues
- **Batch Operations**: Update, delete, or complete multiple issues at once
- **Custom Fields**: Set priority, state, assignee, and any field with validation
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

### Homebrew (macOS/Linux)

```bash
brew tap your-group/track https://gitlab.com/your-group/youtrack-cli.git
brew install track
```

## Quick Start

### YouTrack

```bash
# Initialize (creates .track.toml)
track init --url https://youtrack.example.com --token YOUR_TOKEN

# Set default project
track config project PROJ

# Test connection
track config test

# Get an issue
track PROJ-123

# Search issues
track issue search "project: PROJ #Unresolved" --limit 10
```

### Jira

```bash
# Configure via environment
export JIRA_URL=https://your-domain.atlassian.net
export JIRA_EMAIL=you@example.com
export JIRA_TOKEN=your-api-token

# Or via config file (.track.toml)
[jira]
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"

# Use with -b jira flag
track -b jira PROJ-123
track -b jira issue search "project = PROJ" --limit 10
```

## Configuration

Priority order (highest to lowest):

1. **CLI flags**: `--url`, `--token`, `--backend`
2. **Environment variables**: `TRACKER_URL`, `TRACKER_TOKEN` (or backend-specific)
3. **Config file**: `.track.toml` in project dir, or `~/.config/track/config.toml`

### Environment Variables

```bash
# Generic (any backend)
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

### Config File

```toml
# .track.toml

# YouTrack (default)
url = "https://youtrack.example.com"
token = "perm:base64user.base64name.token"
default_project = "PROJ"

# Jira configuration
[jira]
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"
```

## Backend Selection

Default is YouTrack. You can set the backend in three ways:

### 1. Persistent Configuration (Recommended)

```bash
# Set default backend in config
track config backend jira     # Set to Jira
track config backend youtrack # Set to YouTrack

# Or during init
track init --url https://example.atlassian.net --token XXX --backend jira --email you@example.com
```

### 2. Per-Command Flag

```bash
track -b jira PROJ-123      # Use Jira for this command
track -b j PROJ-123         # Jira (short alias)
track -b yt PROJ-123        # YouTrack (explicit)
```

### 3. Environment Variable

```bash
export TRACKER_BACKEND=jira
track PROJ-123              # Uses Jira
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
track project create -n "Name" -s "KEY"
```

### Tags

```bash
track tags list
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
track cache refresh              # Refresh local cache
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
- Recently accessed issues (LRU, max 50)

### Context (AI-Optimized)

```bash
track context                    # Aggregated context for AI sessions
track context --project PROJ     # Filter to specific project
track context --refresh          # Force refresh from API
track context --include-issues   # Include unresolved issues
track -o json context            # JSON output for parsing
```

Single command to get all relevant data: projects, fields, users, query templates, workflow hints, and recent issues.

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

## Query Syntax

### YouTrack

```bash
track i s "project: PROJ #Unresolved"
track i s "project: PROJ State: {In Progress}"
track i s "project: PROJ Assignee: me Priority: Major"
```

### Jira (JQL)

```bash
track -b j i s "project = PROJ AND resolution IS EMPTY"
track -b j i s "project = PROJ AND status = 'In Progress'"
track -b j i s "assignee = currentUser() AND priority = Major"
```

## Output Formats

```bash
track PROJ-123              # Text (default)
track -o json PROJ-123      # JSON
track --format json p ls    # JSON
```

## Jira Notes

- **Knowledge Base**: Uses Confluence API (automatically at same domain with `/wiki` path)
- **Project Creation**: Requires admin permissions (use web interface)
- **Subtask Conversion**: Create as subtask from start with `--parent`

## Architecture

```
crates/
├── tracker-core/       # Core traits and models
├── youtrack-backend/   # YouTrack API client
├── jira-backend/       # Jira API client
└── track/              # CLI binary
```

- **tracker-core**: `IssueTracker` trait, common models, errors
- **youtrack-backend**: YouTrack REST API with Bearer auth
- **jira-backend**: Jira Cloud REST API v3 with Basic Auth
- **track**: CLI with clap, figment config, text/JSON output

## Development

```bash
# Build
cargo build

# Test
cargo test

# Test specific crate
cargo test --package jira-backend

# Run without installing
cargo run -- PROJ-123
```

## Adding a Backend

1. Create `crates/<backend>-backend/`
2. Implement `IssueTracker` trait
3. Add model conversions
4. Register in `crates/track/src/main.rs`
5. Add config support
6. Add tests with wiremock

See `crates/jira-backend/` for reference.

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
