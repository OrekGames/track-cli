# Track CLI - Agent Guide

> **For AI agents**: Quick reference for programmatic issue tracking. Optimized for fast context retrieval and command generation.

## Quick Context

| Aspect | Details |
|--------|---------|
| **Binary** | `track` (or `target/release/track` if not installed) |
| **Backends** | YouTrack (default), Jira (`-b jira` or `-b j`) |
| **Output** | Text (default) or JSON (`-o json`) |
| **Config** | `.track.toml` in project dir, env vars, or CLI flags |
| **Cache** | `.tracker-cache.json` - run `track cache refresh` for context |

## Backend Comparison

| Feature | YouTrack | Jira |
|---------|----------|------|
| **Flag** | `-b youtrack` or `-b yt` (default) | `-b jira` or `-b j` |
| **Auth** | Bearer token | Basic Auth (email + API token) |
| **Query** | `project: PROJ #Unresolved` | `project = PROJ AND resolution IS EMPTY` (JQL) |
| **Knowledge Base** | Yes (`article` commands) | Yes via Confluence (`article` commands) |
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

### Config Management

```bash
# View/modify configuration
track config show              # Show current config
track config keys              # List all available config keys
track config get <key>         # Get a specific value
track config set <key> <value> # Set a specific value

# Examples
track config set backend jira
track config set jira.email "user@example.com"
track config get default_project
```

**Available config keys**: `backend`, `url`, `token`, `email`, `default_project`, `youtrack.url`, `youtrack.token`, `jira.url`, `jira.email`, `jira.token`

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

### Article Operations (Knowledge Base)

| Operation | YouTrack | Jira/Confluence |
|-----------|----------|-----------------|
| Get | `track a g KB-A-1` | `track -b j a g 123456` |
| List | `track a ls --project PROJ` | `track -b j a ls --project 65957` |
| Search | `track a s "query"` | `track -b j a s "query"` |
| Create | `track a new -p PROJ -s "Title"` | `track -b j a new -p 65957 -s "Title"` |
| Update | `track a u KB-A-1 --summary "New"` | `track -b j a u 123456 --summary "New"` |
| Delete | `track a del KB-A-1` | `track -b j a del 123456` |
| Comments | `track a comments KB-A-1` | `track -b j a comments 123456` |
| Add comment | `track a cmt KB-A-1 -m "Text"` | `track -b j a cmt 123456 -m "Text"` |

**Note**: Confluence uses numeric IDs for both pages and spaces. YouTrack uses readable IDs (e.g., `KB-A-1`).

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

# 2. Refresh cache (recommended - provides comprehensive context)
track cache refresh

# 3. View cached context (includes projects, fields, users, query templates)
track cache show                   # Text output
track -o json cache show           # JSON for parsing

# 4. Get unresolved issues
track -o json i s "project: PROJ #Unresolved" --limit 20     # YouTrack
track -b j -o json i s "project = PROJ AND resolution IS EMPTY" --limit 20  # Jira
```

**Tip**: The cache includes query templates - use `track cache show` to see pre-built queries for common searches.

---

## Cache System

The cache stores comprehensive tracker context locally for fast lookups and AI context:

```bash
track cache refresh       # Fetch and store all cacheable data
track cache show          # Display cached data (text)
track -o json cache show  # JSON format (for programmatic use)
track cache path          # Show cache file location
```

**Cache file** (`.tracker-cache.json`) contains:

| Data | Description |
|------|-------------|
| **Backend metadata** | Backend type and base URL |
| **Default project** | From config, for context |
| **Projects** | IDs, short names, descriptions |
| **Custom fields** | Per project: name, type, required flag, **enum values** |
| **Tags** | Available tags with IDs |
| **Link types** | Issue link types (Relates, Blocks, Depends, etc.) |
| **Query templates** | Pre-built queries per backend (see below) |
| **Project users** | Assignable users per project |
| **Recent issues** | LRU cache of last 50 accessed issues |
| **Articles** | Knowledge base articles with hierarchy |

### Query Templates

The cache includes pre-built query templates for common searches. Use `track cache show` to see them:

| Template | YouTrack Query | Jira JQL |
|----------|---------------|----------|
| `unresolved` | `project: {PROJECT} #Unresolved` | `project = {PROJECT} AND resolution IS EMPTY` |
| `my_issues` | `project: {PROJECT} Assignee: me #Unresolved` | `project = {PROJECT} AND assignee = currentUser() AND resolution IS EMPTY` |
| `recent` | `project: {PROJECT} updated: -7d .. Today` | `project = {PROJECT} AND updated >= -7d` |
| `high_priority` | `project: {PROJECT} Priority: Critical,Major #Unresolved` | `project = {PROJECT} AND priority IN (Highest, High) AND resolution IS EMPTY` |
| `in_progress` | `project: {PROJECT} State: {In Progress}` | `project = {PROJECT} AND status = "In Progress"` |
| `bugs` | `project: {PROJECT} Type: Bug #Unresolved` | `project = {PROJECT} AND issuetype = Bug AND resolution IS EMPTY` |

Replace `{PROJECT}` with the actual project key (e.g., `PROJ`)

---

## Knowledge Base Notes

- **YouTrack**: Built-in Knowledge Base with readable IDs (`KB-A-1`)
- **Jira**: Uses Confluence at same domain (`/wiki` path auto-appended)
- **Confluence IDs**: Numeric for both pages (`123456`) and spaces (`65957`)
- **Discover space IDs**: Run `track -b j a ls` to see existing pages with their space IDs
- **Content**: Supports `--content "text"` or `--content-file ./doc.md`

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
# === SETUP & CONTEXT ===
track config test                  # Test connection
track cache refresh                # Refresh cache (recommended first step)
track cache show                   # View cached context (projects, fields, users, query templates)

# === BACKEND CONFIGURATION ===
track config backend youtrack      # Set default to YouTrack
track config backend jira          # Set default to Jira
track config show                  # Show current config (including backend)
track config keys                  # List all config keys

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

1. **Persistent backend**: `track config backend jira` sets default permanently
2. **Backend override**: `-b jira` or `-b j` overrides per-command
3. **JSON output**: Always use `-o json` for programmatic parsing
4. **Issue shortcut**: `track PROJ-123` = `track issue get PROJ-123`
5. **Default project**: `track config project PROJ` to skip `-p` flag
6. **Field discovery**: `track p f PROJ` or `track cache show` lists custom fields with valid values
7. **Cache context**: `track cache refresh` fetches projects, fields, users, link types, query templates, and articles
8. **Query templates**: Cache includes pre-built queries - check `track cache show` for available templates
9. **Jira limitations**: No project creation, no subtask conversion
10. **Query syntax**: YouTrack `project: PROJ` vs Jira `project = PROJ`
11. **Confluence IDs**: Numeric page IDs (`123456`) and space IDs (`65957`), not project keys
