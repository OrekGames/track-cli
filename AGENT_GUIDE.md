# Agent Guide: Using the Track CLI

This guide is for AI agents (Claude Code, Cursor, etc.) that need to interact with issue tracking systems during coding sessions. The `track` CLI provides a programmatic interface to YouTrack (and future backends) with JSON output for easy parsing.

## Setup

The binary is located at `target/release/track`. Configure it using one of:

1. **Config file**: `--config ./target/release/config.toml`
2. **Environment variables**: `TRACKER_URL`, `TRACKER_TOKEN`
3. **CLI flags**: `--url`, `--token`

```bash
# Set up alias for convenience
TRACK="./target/release/track --config ./target/release/config.toml"
```

## Command Aliases

All commands have short aliases for faster typing:

| Command | Aliases |
|---------|---------|
| `track issue` | `track i` |
| `track issue get` | `track i g` |
| `track issue create` | `track i new`, `track i c` |
| `track issue update` | `track i u` |
| `track issue search` | `track i s`, `track i find` |
| `track issue delete` | `track i rm`, `track i del` |
| `track issue comment` | `track i cmt` |
| `track issue complete` | `track i done`, `track i resolve` |
| `track project` | `track p` |
| `track project list` | `track p ls` |
| `track project fields` | `track p f` |
| `track tags` | `track t` |
| `track tags list` | `track t ls` |
| `track config` | `track cfg` |
| `track config project` | `track cfg proj` |

## Quick Reference

```bash
# Set default project (only needs to be done once)
$TRACK config project OGIT

# List projects (shortNames now auto-resolve to internal IDs!)
$TRACK -o json p ls

# Get issue details
$TRACK -o json i g OGIT-123

# Get issue with full context (subtasks, links, comments)
$TRACK i g OGIT-123 --full

# Create issue (uses default project if -p not specified)
$TRACK i new -s "Summary" -d "Description" --priority "Normal"

# Create issue with explicit project
$TRACK i new -p OGIT -s "Summary" --priority "Normal"

# Update issue
$TRACK i u OGIT-123 --field "Stage=In Progress"
$TRACK i u OGIT-123 --field "Stage=Done" --priority "Minor"

# Quick state transitions
$TRACK i start OGIT-123           # Set to in-progress (Stage=Develop)
$TRACK i complete OGIT-123        # Set to done (Stage=Done)

# Add/view comments
$TRACK i comment OGIT-123 -m "Work in progress notes"
$TRACK i comments OGIT-123        # List comments

# Link issues
$TRACK i link OGIT-123 OGIT-456               # Relates (default)
$TRACK i link OGIT-123 OGIT-456 -t depends    # Depends on

# Search issues
$TRACK -o json i s "project: OGIT #Unresolved" --limit 20

# List available tags
$TRACK -o json t ls

# Get custom fields for a project
$TRACK p f OGIT
```

## Cache System (Recommended for AI Sessions)

The cache stores tracker context locally so agents can understand available projects, fields, and tags without making repeated API calls.

```bash
# Refresh cache (run this once at session start)
$TRACK cache refresh

# View cached context
$TRACK cache show           # Human-readable
$TRACK -o json cache show   # JSON for programmatic access

# Find cache file location
$TRACK cache path
```

The cache file (`.tracker-cache.json`) contains:
- All projects with their internal IDs and shortNames
- Custom fields for each project (with types and required flags)
- All available tags with their IDs

**Workflow tip**: Run `track cache refresh` at the start of a session, then read the cache file directly for context without API calls.

## Discovering Custom Fields

Use `track p f <project>` to discover custom fields for a project:

```bash
$TRACK p f OGIT
# Output:
# Custom fields for project OGIT:
#   Priority [enum[1]] (required)
#   Assignee [user[1]]
#   Kanban State [enum[1]]
#   Stage [state[1]] (required)
```

### Example Custom Field Values (OGIT Project)

- **Priority** (SingleEnum): Normal, Major, Minor, Critical
- **Assignee** (SingleUser): user login
- **Kanban State** (SingleEnum): Ready to pull, In Progress, etc.
- **Stage** (State): Backlog, In Progress, Done, etc.

## Workflows

### Starting Work on an Issue

```bash
# 1. Find the issue
$TRACK -o json i s "project: OGIT summary:~'feature name'" --limit 5

# 2. View issue details with full context (shows subtasks, links, comments)
$TRACK i g OGIT-XX --full

# 3. Start work (quick shortcut for setting state to in-progress)
$TRACK i start OGIT-XX

# Or manually set fields
$TRACK i u OGIT-XX --field "Stage=Develop" --field "Kanban State=In Progress"
```

### Completing Work

```bash
# Quick completion (sets Stage=Done)
$TRACK i complete OGIT-XX
# Or use aliases: track i done OGIT-XX

# With custom state values
$TRACK i complete OGIT-XX --state "Resolved"

# Or manually with additional context
$TRACK i u OGIT-XX --field "Stage=Done" --description "Completed: implemented X, Y, Z. See commit abc123."
```

### Adding Comments

```bash
# Add a comment to an issue
$TRACK i comment OGIT-XX -m "Started implementation, found edge case in auth flow"
$TRACK i cmt OGIT-XX -m "Fixed the edge case, tests passing"

# View comments on an issue
$TRACK i comments OGIT-XX
$TRACK i comments OGIT-XX --limit 5
```

### Linking Issues

```bash
# Link two issues as related (default)
$TRACK i link OGIT-XX OGIT-YY

# Create dependency link (XX depends on YY)
$TRACK i link OGIT-XX OGIT-YY -t depends

# Mark as duplicate
$TRACK i link OGIT-XX OGIT-YY -t duplicates

# Link type options: relates, depends, required, duplicates, duplicated-by, subtask, parent
```

### Creating a New Issue

```bash
# With default project set, no need for -p flag
$TRACK i new -s "Implement new feature" \
  -d "Detailed description of what needs to be done" \
  --priority "Normal" \
  --field "Kanban State=Ready to pull"

# Or explicitly specify project
$TRACK i new -p OGIT -s "Feature for different project" --priority "Normal"
```

### Creating Subtasks

```bash
# Create a subtask of an existing issue
$TRACK i new -s "Implement component A" --parent OGIT-45 --priority "Normal"

# Multiple subtasks for breaking down work
$TRACK i new -s "Write unit tests" --parent OGIT-45
$TRACK i new -s "Update documentation" --parent OGIT-45
$TRACK i new -s "Code review fixes" --parent OGIT-45
```

## Session Startup Checklist

1. **Set default project** (once, persists across sessions):
   ```bash
   $TRACK config project OGIT
   $TRACK config show          # Verify it's set
   ```

2. **Refresh cache** (once per session):
   ```bash
   $TRACK cache refresh
   ```

3. **Read cache for context** (optional - understand available fields):
   ```bash
   $TRACK -o json cache show
   ```

4. **Check unresolved issues** (for context on current work):
   ```bash
   $TRACK -o json i s "project: OGIT #Unresolved" --limit 20
   ```

## Important Notes

1. **Default Project**: Set with `track config project <ID>` - then `-p` is optional for issue creation
2. **Subtasks**: Use `--parent ISSUE-ID` to create an issue as a subtask of another issue
3. **Project ShortNames**: The CLI auto-resolves shortNames (OGIT) to internal IDs (0-2)
4. **Field Names**: Field names are case-sensitive and project-specific. Use `track p f <project>` to discover them.
5. **JSON Output**: Use `-o json` when parsing output programmatically
6. **Tags**: Tags must already exist in the tracker. Use `track t ls` to see available tags.
7. **Full Context**: Use `--full` with `issue get` to see subtasks, links, and comments in one view
8. **Quick Transitions**: Use `start` and `complete` commands for common state changes. Customize state values with `--field` and `--state` flags.
9. **Link Types**: Available types are `relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, `parent`
10. **Error Messages**: Include full error chain context for debugging

## Output Formats

- **Text** (default): Human-readable output for interactive use
- **JSON** (`-o json`): Machine-readable output for programmatic parsing

Always use `-o json` when you need to parse the output programmatically.
