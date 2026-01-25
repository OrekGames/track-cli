# Tracker CLI

A command-line interface for interacting with issue tracking systems, built with Rust. Currently supports YouTrack, with architecture designed for multi-backend support (Jira, Linear, etc.).

## Features

- **Issue Management**: Get, create, update, delete, and search issues
- **Custom Fields**: Set priority, state, assignee, and any custom field on create/update
- **Tags**: Add existing tags to issues
- **Project Operations**: List and get project details
- **Comments & Links**: Add comments and link issues together
- **Multiple Output Formats**: Text (human-readable) and JSON (machine-readable)
- **Flexible Configuration**: CLI flags, environment variables, or config file
- **Multi-Backend Architecture**: Designed to support multiple issue trackers
- **Fast & Reliable**: Built with Rust for performance and safety

## Installation

### Homebrew (macOS/Linux)

For private GitLab repositories, first set up authentication:

```bash
# Add to ~/.zshrc or ~/.bashrc
export GITLAB_TOKEN="glpat-xxxxxxxxxxxxxxxxxxxx"
```

Then install:

```bash
brew tap your-group/track https://gitlab.com/your-group/youtrack-cli.git
brew install track
```

### Cargo (from source)

```bash
cargo install --path crates/track
```

### Build from source

```bash
cargo build --release
# Binary will be at target/release/track
```

### Download Binary

Download pre-built binaries from the [Releases](https://gitlab.com/your-group/youtrack-cli/-/releases) page.

## Configuration

Configure your tracker instance in three ways (priority from highest to lowest):

1. **CLI flags**:
   ```bash
   track --url https://youtrack.example.com --token YOUR_TOKEN issue get PROJ-123
   ```

2. **Environment variables**:
   ```bash
   # Generic (works for any backend)
   export TRACKER_URL=https://youtrack.example.com
   export TRACKER_TOKEN=YOUR_TOKEN

   # Or backend-specific (YouTrack)
   export YOUTRACK_URL=https://youtrack.example.com
   export YOUTRACK_TOKEN=YOUR_TOKEN

   track issue get PROJ-123
   ```

3. **Config file** (TOML):
   - Default search order: OS config dir (`~/.config/track/config.toml` on Linux, `~/Library/Application Support/track/config.toml` on macOS), then `./config.toml`
   - Or a specific file via `--config PATH` / `TRACKER_CONFIG`

   ```toml
   # Global settings (applies to all backends)
   url = "https://youtrack.example.com"
   token = "perm:base64user.base64name.token"

   # Or backend-specific settings
   [youtrack]
   url = "https://youtrack.example.com"
   token = "perm:base64user.base64name.token"

   # Future: Jira support
   # [jira]
   # url = "https://jira.example.com"
   # token = "..."
   ```

## Quick Start

```bash
# Initialize configuration (creates .track.toml in current directory)
track init --url https://youtrack.example.com --token YOUR_TOKEN

# Set a default project
track config project PROJ

# Test your connection
track config test

# View an issue (shortcut - no subcommand needed!)
track PROJ-123

# Open issue in browser
track open PROJ-123

# Create an issue
track issue create -s "Bug summary" --priority "Major"
```

## Usage

### Backend Selection

By default, the CLI uses YouTrack. When additional backends are added, you can switch:

```bash
track --backend youtrack issue get PROJ-123  # Explicit YouTrack
track -b youtrack issue get PROJ-123         # Short form
# Future: track --backend jira issue get PROJ-123
```

### Shortcuts

**Direct issue access** - Skip the `issue get` subcommand:
```bash
track PROJ-123           # Same as: track issue get PROJ-123
track PROJ-123 --full    # Same as: track issue get PROJ-123 --full
```

**Open in browser:**
```bash
track open PROJ-123      # Opens issue in default browser
track open               # Opens the tracker dashboard
```

### Command Aliases

All commands have short aliases for faster typing:

| Command | Aliases | Description |
|---------|---------|-------------|
| `track PROJ-123` | - | Shortcut for `track issue get PROJ-123` |
| `track open PROJ-123` | - | Open issue in browser |
| `track issue` | `track i` | Issue operations |
| `track issue get` | `track i g` | Get issue details |
| `track issue create` | `track i new`, `track i c` | Create new issue |
| `track issue update` | `track i u` | Update issue |
| `track issue search` | `track i s`, `track i find` | Search issues |
| `track issue delete` | `track i rm`, `track i del` | Delete issue |
| `track issue comment` | `track i cmt` | Add comment |
| `track issue complete` | `track i done`, `track i resolve` | Complete issue |
| `track project` | `track p` | Project operations |
| `track project list` | `track p ls` | List projects |
| `track project fields` | `track p f` | List custom fields |
| `track tags` | `track t` | Tag operations |
| `track tags list` | `track t ls` | List tags |
| `track config` | `track cfg` | Configuration |
| `track config project` | `track cfg proj` | Set default project |
| `track config test` | - | Test connection |

### Issue Commands

**Get an issue:**
```bash
track issue get PROJ-123
track i g PROJ-123          # Using aliases
track -o json i g PROJ-123  # JSON output
track i g PROJ-123 --full   # Full context (links, comments)
```

**Create an issue (basic):**
```bash
track issue create -p PROJECT_ID -s "Bug in login" -d "Users can't log in"
```

**Create an issue with custom fields:**
```bash
# Using dedicated flags for common fields
track issue create -p PROJECT_ID -s "Fix bug" --priority "Major" --assignee "john.doe"

# Using generic --field flag for any custom field
track issue create -p PROJECT_ID -s "New feature" --field "Priority=Critical" --field "Type=Feature"

# Combining multiple options
track issue create -p PROJECT_ID -s "Urgent fix" -d "Description here" \
  --priority "Critical" \
  --field "Kanban State=In Progress"
```

**Create a subtask:**
```bash
# Create an issue as a subtask of another issue
track issue create -p PROJ -s "Subtask summary" --parent PROJ-123

# Subtask with default project (if configured)
track issue create -s "Fix component X" --parent PROJ-100 --priority "Normal"
```

**Update an issue:**
```bash
# Update summary
track issue update PROJ-123 --summary "Updated summary"

# Update custom fields
track issue update PROJ-123 --priority "Minor"
track issue update PROJ-123 --field "Stage=Done"

# Multiple updates at once
track issue update PROJ-123 --summary "New title" --priority "Major" --field "Kanban State=Done"
```

**Quick state transitions:**
```bash
track issue start PROJ-123     # Set to in-progress
track issue complete PROJ-123  # Set to done
track i done PROJ-123          # Alias for complete
```

**Comments:**
```bash
track issue comment PROJ-123 -m "Started working on this"
track issue comments PROJ-123 --limit 10  # View comments
```

**Link issues:**
```bash
track issue link PROJ-123 PROJ-456              # Relates (default)
track issue link PROJ-123 PROJ-456 -t depends   # Depends on
track issue link PROJ-123 PROJ-456 -t subtask   # Mark as subtask
```

**Search issues:**
```bash
track issue search "project: PROJ #Unresolved" --limit 10
track issue search "project: PROJ State: Open" --limit 20 --skip 0
```

**Delete an issue:**
```bash
track issue delete PROJ-123
```

### Project Commands

**List all projects:**
```bash
track project list
track p ls                   # Using alias
track -o json p ls           # Get project IDs for API calls
```

**Get a project:**
```bash
track project get PROJ       # Works with shortName or internal ID
track p g OGIT               # Using alias
```

**List custom fields for a project:**
```bash
track project fields OGIT    # Discover available custom fields
track p f OGIT               # Using alias
```

### Tag Commands

**List all available tags:**
```bash
track tags list
track t ls                   # Using alias
track -o json t ls           # Get tag IDs for API calls
```

### Cache Commands

The cache stores tracker context locally for AI assistants to read without making API calls.

**Refresh the cache:**
```bash
track cache refresh          # Fetches projects, fields, and tags
```

**Show cached context:**
```bash
track cache show             # Human-readable format
track -o json cache show     # JSON for programmatic access
```

**Show cache file path:**
```bash
track cache path             # Shows where cache is stored
```

### Initialization

Initialize a local `.track.toml` config file in your project directory:

```bash
# Basic initialization
track init --url https://youtrack.example.com --token YOUR_TOKEN

# Initialize with default project
track init --url https://youtrack.example.com --token YOUR_TOKEN --project PROJ
```

This creates a `.track.toml` file that stores your credentials locally, so you don't need to specify `--url` and `--token` for every command.

### Config Commands

Manage local configuration settings.

**Test connection:**
```bash
track config test            # Validates URL and token work correctly
```

**Set default project:**
```bash
track config project OGIT    # Set default using shortName
track cfg proj 0-2           # Or using internal ID
```

**Show current configuration:**
```bash
track config show            # Shows URL, token status, default project
track -o json config show    # JSON format
```

**Clear default project:**
```bash
track config clear           # Remove default project (keeps URL/token)
```

**Show config file path:**
```bash
track config path            # Shows .track.toml location
```

Once a default project is set, you can create issues without `-p`:
```bash
track issue create -s "Fix bug" --priority "Major"  # Uses default project
```

### Browser Commands

Open issues or the tracker dashboard in your default browser:

```bash
track open PROJ-123          # Open specific issue
track open                   # Open tracker dashboard
```

### Output Formats

**JSON output** (for scripts and AI assistants):
```bash
track --format json issue get PROJ-123
track -o json project list
```

**Text output** (default, human-readable):
```bash
track issue get PROJ-123
```

---

## Architecture

The project is structured as a Rust workspace with three crates:

- **`tracker-core`**: Core abstractions for issue tracking
  - `IssueTracker` trait defining common operations
  - Backend-agnostic models (Issue, Project, Comment, etc.)
  - Common error types (`TrackerError`)

- **`youtrack-backend`**: YouTrack API implementation
  - HTTP client using `ureq` (synchronous, no async overhead)
  - YouTrack-specific Serde models for API serialization
  - Implements `IssueTracker` trait for YouTrackClient
  - Full unit test coverage with `wiremock`

- **`track`**: CLI binary
  - Argument parsing with `clap`
  - Configuration management with `figment`
  - Backend selection via `--backend` flag
  - Output formatting (text and JSON)
  - Integration tests with `assert_cmd`

```
tracker-cli/
├── Cargo.toml                      # Workspace manifest
├── crates/
│   ├── tracker-core/               # Core traits and models
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs           # IssueTracker trait
│   │       ├── models.rs           # Common types
│   │       └── error.rs            # TrackerError
│   │
│   ├── youtrack-backend/           # YouTrack implementation
│   │   └── src/
│   │       ├── client.rs           # YouTrackClient
│   │       ├── convert.rs          # Model conversions
│   │       ├── trait_impl.rs       # impl IssueTracker
│   │       └── models/             # YouTrack API models
│   │
│   └── track/                      # CLI binary
│       └── src/
│           ├── main.rs
│           ├── cli.rs              # Clap definitions
│           ├── config.rs           # Multi-backend config
│           └── commands/           # Command handlers
```

## Development

**Run tests:**
```bash
cargo test
```

**Run specific tests:**
```bash
cargo test --package tracker-core
cargo test --package youtrack-backend
cargo test --package track
```

**Build:**
```bash
cargo build --release
```

**Run without installing:**
```bash
cargo run -- issue get PROJ-123
```

## Adding a New Backend

To add support for a new issue tracker (e.g., Jira):

1. Create a new crate: `crates/jira-backend/`
2. Implement `IssueTracker` trait for your client
3. Add conversion functions for models
4. Register the backend in `crates/track/src/main.rs`
5. Add configuration support in `config.rs`

The CLI commands will work automatically with the new backend.

## Dependencies

Key dependencies:

- `ureq 3.1` - Synchronous HTTP client
- `clap 4.5` - Command-line argument parser
- `serde 1.0` - Serialization/deserialization
- `figment 0.10` - Configuration management
- `thiserror 2.0` - Error handling for library
- `anyhow 1.0` - Error handling for binary
- `chrono 0.4` - Date/time handling
- `wiremock 0.6` - HTTP mocking for tests
- `assert_cmd 2.0` - CLI integration tests

## License

MIT
