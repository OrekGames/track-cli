# Track CLI - Agent Guide

> **Purpose**: Unified CLI for issue tracking systems (YouTrack, Jira). Use this tool to manage issues, projects, comments, and links programmatically during coding sessions.

## Quick Context

| Aspect | Details |
|--------|---------|
| **Binary** | `track` (or `target/release/track` if not installed) |
| **Backends** | YouTrack (default), Jira (`-b jira` or `-b j`) |
| **Output** | Text (default) or JSON (`-o json`) |
| **Config** | `.track.toml` in project dir, env vars, or CLI flags |

## Backend Comparison

| Feature | YouTrack | Jira |
|---------|----------|------|
| **Flag** | `-b youtrack` or `-b yt` (default) | `-b jira` or `-b j` |
| **Auth** | Bearer token | Basic Auth (email + API token) |
| **Query** | `project: PROJ #Unresolved` | `project = PROJ AND resolution IS EMPTY` (JQL) |
| **Knowledge Base** | Yes (`article` commands) | No (uses Confluence) |
| **Project Creation** | Yes | No (admin only) |

## Configuration

### YouTrack

```bash
# Initialize with persistent config
track init --url https://youtrack.example.com --token YOUR_TOKEN --project PROJ

# Or environment variables
export YOUTRACK_URL=https://youtrack.example.com
export YOUTRACK_TOKEN=perm:xxx

# Or config file (.track.toml)
backend = "youtrack"
url = "https://youtrack.example.com"
token = "perm:xxx"
default_project = "PROJ"
```

### Jira

```bash
# Initialize with persistent config
track init --url https://your-domain.atlassian.net --token API_TOKEN --backend jira --email you@example.com

# Or environment variables
export JIRA_URL=https://your-domain.atlassian.net
export JIRA_EMAIL=you@example.com
export JIRA_TOKEN=your-api-token

# Or config file (.track.toml)
backend = "jira"
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"
```

### Switching Backends

```bash
# Set default backend persistently
track config backend jira      # Switch to Jira
track config backend youtrack  # Switch to YouTrack

# Override per-command
track -b jira PROJ-123         # Use Jira for this command only
```

### Test Connection

```bash
track config test              # Uses configured backend
track -b jira config test      # Override to test Jira
```

---

## Command Reference

### Issue Operations

| Operation | YouTrack | Jira |
|-----------|----------|------|
| Get issue | `track PROJ-123` | `track -b j PROJ-123` |
| Get (JSON) | `track -o json PROJ-123` | `track -b j -o json PROJ-123` |
| Get (full) | `track PROJ-123 --full` | `track -b j PROJ-123 --full` |
| Create | `track i new -p PROJ -s "Summary"` | `track -b j i new -p PROJ -s "Summary"` |
| Update | `track i u PROJ-123 --summary "New"` | `track -b j i u PROJ-123 --summary "New"` |
| Delete | `track i del PROJ-123` | `track -b j i del PROJ-123` |
| Search | `track i s "project: PROJ #Unresolved"` | `track -b j i s "project = PROJ"` |
| Comment | `track i cmt PROJ-123 -m "Text"` | `track -b j i cmt PROJ-123 -m "Text"` |
| Link | `track i link PROJ-1 PROJ-2` | `track -b j i link PROJ-1 PROJ-2` |

### Project Operations

| Operation | YouTrack | Jira |
|-----------|----------|------|
| List | `track p ls` | `track -b j p ls` |
| Get | `track p g PROJ` | `track -b j p g PROJ` |
| Fields | `track p f PROJ` | `track -b j p f PROJ` |
| Create | `track p new -n "Name" -s "KEY"` | Not supported |

### Command Aliases

| Full Command | Aliases |
|--------------|---------|
| `track issue` | `track i` |
| `track issue get` | `track i g` |
| `track issue create` | `track i new`, `track i c` |
| `track issue update` | `track i u` |
| `track issue search` | `track i s`, `track i find` |
| `track issue delete` | `track i rm`, `track i del` |
| `track issue comment` | `track i cmt` |
| `track issue comments` | `track i comments` |
| `track issue complete` | `track i done`, `track i resolve` |
| `track issue start` | `track i start` |
| `track issue link` | `track i link` |
| `track project` | `track p` |
| `track project list` | `track p ls` |
| `track project get` | `track p g` |
| `track project fields` | `track p f` |
| `track tags list` | `track t ls` |
| `track config` | `track cfg` |
| `track article` | `track a`, `track wiki` |

---

## Search Query Syntax

### YouTrack Query Examples

```bash
# Unresolved issues in project
track i s "project: PROJ #Unresolved" --limit 20

# By state
track i s "project: PROJ State: {In Progress}"

# By assignee
track i s "project: PROJ Assignee: me"

# Combined
track i s "project: PROJ #Unresolved Priority: Major"
```

### Jira JQL Examples

```bash
# Unresolved issues in project
track -b j i s "project = PROJ AND resolution IS EMPTY" --limit 20

# By status
track -b j i s "project = PROJ AND status = 'In Progress'"

# By assignee
track -b j i s "assignee = currentUser()"

# Combined
track -b j i s "project = PROJ AND resolution IS EMPTY AND priority = Major"
```

### Query Syntax Comparison

| Concept | YouTrack | Jira JQL |
|---------|----------|----------|
| Project filter | `project: PROJ` | `project = PROJ` |
| Unresolved | `#Unresolved` | `resolution IS EMPTY` |
| Resolved | `#Resolved` | `resolution IS NOT EMPTY` |
| Open status | `State: Open` | `status = Open` |
| In progress | `State: {In Progress}` | `status = "In Progress"` |
| Current user | `Assignee: me` | `assignee = currentUser()` |
| Priority | `Priority: Major` | `priority = Major` |
| Text search | `summary:~'keyword'` | `summary ~ "keyword"` |
| AND | implicit or `AND` | `AND` |
| OR | `OR` | `OR` |

---

## Common Workflows

### Get Issue Details

```bash
# YouTrack
track PROJ-123                    # Basic info
track PROJ-123 --full             # With comments, links, subtasks
track -o json PROJ-123            # JSON for parsing

# Jira
track -b j PROJ-123
track -b j PROJ-123 --full
track -b j -o json PROJ-123
```

### Create Issue

```bash
# YouTrack
track i new -p PROJ -s "Bug title" -d "Description" --priority "Major"
track i new -s "Subtask" --parent PROJ-100   # Subtask

# Jira
track -b j i new -p PROJ -s "Bug title" -d "Description"
track -b j i new -s "Subtask" --parent PROJ-100
```

### Update Issue

```bash
# YouTrack
track i u PROJ-123 --summary "New title"
track i u PROJ-123 --field "Priority=Critical"
track i u PROJ-123 --field "Stage=Done"

# Jira
track -b j i u PROJ-123 --summary "New title"
track -b j i u PROJ-123 --description "Updated description"
```

### State Transitions

```bash
# YouTrack - Quick commands
track i start PROJ-123       # Set to in-progress
track i complete PROJ-123    # Set to done

# YouTrack - Manual field update
track i u PROJ-123 --field "Stage=In Progress"

# Jira - Manual update (no quick commands)
track -b j i u PROJ-123 --field "status=In Progress"
```

### Comments

```bash
# Add comment
track i cmt PROJ-123 -m "Started implementation"
track -b j i cmt PROJ-123 -m "Started implementation"

# List comments
track i comments PROJ-123
track -b j i comments PROJ-123
```

### Link Issues

```bash
# YouTrack
track i link PROJ-1 PROJ-2               # Relates (default)
track i link PROJ-1 PROJ-2 -t depends    # Depends on
track i link PROJ-1 PROJ-2 -t subtask    # Subtask link

# Jira
track -b j i link PROJ-1 PROJ-2
track -b j i link PROJ-1 PROJ-2 -t Blocks
```

**Link types**: `relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, `parent`

---

## Session Startup

```bash
# 1. Verify connection
track config test                  # YouTrack
track -b jira config test          # Jira

# 2. List projects (get context)
track -o json p ls

# 3. Get unresolved issues
track -o json i s "project: PROJ #Unresolved" --limit 20     # YouTrack
track -b j -o json i s "project = PROJ AND resolution IS EMPTY" --limit 20  # Jira

# 4. (Optional) Refresh cache for offline context
track cache refresh
```

---

## Cache System

The cache stores project metadata locally for fast context retrieval:

```bash
track cache refresh    # Fetch and store projects, fields, tags
track cache show       # Display cached data
track -o json cache show  # JSON format
track cache path       # Show cache file location
```

**Cache file** (`.tracker-cache.json`) contains:
- Projects with IDs and short names
- Custom fields per project (types, required flags)
- Available tags

---

## Knowledge Base (YouTrack Only)

```bash
# Get article
track a g KB-A-1

# List articles
track a ls --project PROJ --limit 20

# Search
track a s "search term" --limit 10

# Create
track a new --project PROJ --summary "Title" --content "Body text"
track a new --project PROJ --summary "Title" --content-file ./doc.md

# Update
track a u KB-A-1 --summary "New Title"
track a u KB-A-1 --content-file ./updated.md

# Delete
track a del KB-A-1
```

---

## Output Formats

```bash
# Text output (default) - human readable
track PROJ-123

# JSON output - for programmatic parsing
track -o json PROJ-123
track --format json p ls
```

**Always use `-o json` when parsing output programmatically.**

---

## Error Handling

Common errors and solutions:

| Error | Cause | Solution |
|-------|-------|----------|
| `Unauthorized` | Invalid/expired token | Check token, regenerate if needed |
| `Not found` | Issue/project doesn't exist | Verify ID, check project access |
| `Project not found` | Invalid project key | Use `track p ls` to list valid projects |
| `Connection refused` | Wrong URL or network issue | Verify URL, check network |

---

## Quick Reference Card

```bash
# === BACKEND CONFIGURATION ===
track config backend youtrack      # Set default to YouTrack
track config backend jira          # Set default to Jira
track config show                  # Show current config (including backend)

# === YOUTRACK (when default or with -b yt) ===
track PROJ-123                     # Get issue
track -o json PROJ-123             # Get as JSON
track i s "project: PROJ #Unresolved" --limit 20
track i new -p PROJ -s "Summary" --priority "Normal"
track i u PROJ-123 --field "Stage=Done"
track i cmt PROJ-123 -m "Comment"
track i link PROJ-1 PROJ-2 -t depends
track p ls                         # List projects
track config test                  # Test connection

# === JIRA (when default or with -b j) ===
track -b j PROJ-123                # Get issue (or just 'track PROJ-123' if default is jira)
track -b j -o json PROJ-123        # Get as JSON
track -b j i s "project = PROJ AND resolution IS EMPTY" --limit 20
track -b j i new -p PROJ -s "Summary"
track -b j i u PROJ-123 --summary "New title"
track -b j i cmt PROJ-123 -m "Comment"
track -b j i link PROJ-1 PROJ-2
track -b j p ls                    # List projects
track -b j config test             # Test connection
```

---

## Important Notes

1. **Persistent backend**: Use `track config backend jira` to set default backend permanently.
2. **Backend override**: Use `-b jira` or `-b j` to override the configured backend per-command.
3. **JSON output**: Always use `-o json` for programmatic parsing.
4. **Issue shortcut**: `track PROJ-123` = `track issue get PROJ-123`
5. **Default project**: Set with `track config project PROJ` to skip `-p` flag.
6. **Field discovery**: Use `track p f PROJ` to see available custom fields.
7. **Jira limitations**: No knowledge base, no project creation, no subtask conversion.
8. **Query syntax differs**: YouTrack uses `project: PROJ`, Jira uses `project = PROJ`.
