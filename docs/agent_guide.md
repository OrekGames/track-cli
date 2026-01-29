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
| **AI Context** | `track context` - aggregated context in single command |

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
| Create (validate) | `track i new -p PROJ -s "Summary" --validate` | Same with `-b j` |
| Update | `track i u PROJ-123 --summary "New"` | `track -b j i u PROJ-123 --summary "New"` |
| Update (validate) | `track i u PROJ-123 --field "State=Done" --validate` | Same with `-b j` |
| Delete | `track i del PROJ-123` | `track -b j i del PROJ-123` |
| Search | `track i s "project: PROJ #Unresolved"` | `track -b j i s "project = PROJ"` |
| Search (template) | `track i s -T unresolved -p PROJ` | Same with `-b j` |
| Comment | `track i cmt PROJ-123 -m "Text"` | `track -b j i cmt PROJ-123 -m "Text"` |
| Link | `track i link PROJ-1 PROJ-2` | `track -b j i link PROJ-1 PROJ-2` |

### Project Operations

| Operation | YouTrack | Jira |
|-----------|----------|------|
| List | `track p ls` | `track -b j p ls` |
| Get | `track p g PROJ` | `track -b j p g PROJ` |
| Fields | `track p f PROJ` | `track -b j p f PROJ` |
| Create | `track p new -n "Name" -s "KEY"` | Not supported |
| Attach field | `track p attach-field PROJ -f <field-id> --bundle <bundle-id>` | Not supported |

### Custom Field Admin (YouTrack only)

| Operation | Command |
|-----------|---------|
| List fields | `track field list` or `track field ls` |
| Create field | `track field create "Name" -t enum` |
| Create field with values + attach | `track field new "Name" -t enum -p PROJ -v "Val1,Val2,Val3"` |
| List bundles | `track bundle list -t enum` |
| Create bundle | `track bundle create "Name" -t enum -v "Val1,Val2"` |
| Add bundle value | `track bundle add-value <id> -t enum -v "NewValue"` |

**Field types**: `enum`, `multi-enum`, `state`, `text`, `date`, `integer`, `float`, `period`

**Bundle types**: `enum`, `state`, `ownedField`, `version`, `build`

**State fields with resolved markers**:
```bash
track field new "Status" -t state -p PROJ -v "Open,In Progress,Done" --resolved "Done"
track bundle create "Bug Status" -t state -v "Open,Fixed,Closed" --resolved "Fixed,Closed"
```

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
| `track context` | `track ctx` |
| `track field list` | `track field ls` |
| `track field create` | `track field c` |
| `track field new` | `track field setup` |
| `track bundle list` | `track bundle ls` |
| `track bundle create` | `track bundle c` |

---

## Batch Operations

Update, delete, start, or complete multiple issues in a single command using comma-separated IDs.

### Batch Update

```bash
# Update multiple issues at once
track i u PROJ-1,PROJ-2,PROJ-3 --field "Priority=Major"

# Update state for multiple issues
track i u PROJ-1,PROJ-2 --state "In Progress"

# Validate before batch update
track i u PROJ-1,PROJ-2 --field "State=Done" --validate
```

### Batch Start/Complete

```bash
# Start work on multiple issues
track i start PROJ-1,PROJ-2,PROJ-3

# Complete multiple issues
track i done PROJ-1,PROJ-2 --state Done
```

### Batch Delete

```bash
# Delete multiple issues
track i del PROJ-1,PROJ-2,PROJ-3
```

### Batch Output Format

Text output shows success/failure summary:
```
✓ 3 issues updated
  ✓ PROJ-1
  ✓ PROJ-2
  ✓ PROJ-3
```

Partial failures show both:
```
⚠ 2/3 issues updated (1 failed)
  ✓ PROJ-1
  ✗ PROJ-2 - Invalid value for field 'State'
  ✓ PROJ-3
```

JSON output provides structured results:
```json
{
  "total": 3,
  "succeeded": 2,
  "failed": 1,
  "results": [
    {"id": "PROJ-1", "success": true, "id_readable": "PROJ-1"},
    {"id": "PROJ-2", "success": false, "error": "Invalid value..."},
    {"id": "PROJ-3", "success": true, "id_readable": "PROJ-3"}
  ]
}
```

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

# 2. Get aggregated context (recommended - single command for all AI context)
track context                      # Full context from cache
track context --refresh            # Force refresh from API
track context --include-issues     # Include unresolved issues
track -o json context              # JSON for parsing

# Alternative: Refresh cache separately
track cache refresh

# 3. View cached context (includes projects, fields, users, query templates)
track cache show                   # Text output
track -o json cache show           # JSON for parsing

# 4. Get unresolved issues
track -o json i s "project: PROJ #Unresolved" --limit 20     # YouTrack
track -b j -o json i s "project = PROJ AND resolution IS EMPTY" --limit 20  # Jira
```

**Tip**: Use `track context -o json` for a single-command context dump optimized for AI sessions.

### Cache Freshness

Check cache age and conditionally refresh:

```bash
# Check cache status (age, freshness, data counts)
track cache status
track -o json cache status

# Conditional refresh - only if cache is older than duration
track cache refresh --if-stale 1h      # Refresh if older than 1 hour
track cache refresh --if-stale 30m     # Refresh if older than 30 minutes
track cache refresh --if-stale 1d      # Refresh if older than 1 day

# Force refresh (always)
track cache refresh
```

**Duration formats**: `1h` (hours), `30m` (minutes), `1d` (days), `60s` (seconds)

---

## AI-Optimized Features

### Context Command

Aggregates all relevant data for AI sessions in a single call:

```bash
track context                        # Full context (projects, fields, users, templates)
track context --project PROJ         # Filter to specific project
track context --refresh              # Force refresh from API
track context --include-issues       # Include unresolved issues
track context --issue-limit 25       # Limit included issues (default: 10)
track -o json context                # JSON output for parsing
```

**Output includes**: Backend info, projects, custom fields with enum values, tags, link types, query templates, assignable users, workflow hints (state transitions), recent issues.

### Template-Based Search

Use pre-built query templates instead of raw queries (avoids backend syntax errors):

```bash
# Use template with project
track i s --template unresolved --project PROJ
track i s -T my_issues -p PROJ

# Available templates (see: track cache show)
# - unresolved: All unresolved issues
# - my_issues: Assigned to current user
# - recent: Recently updated (7 days)
# - high_priority: Critical/Major priority
# - in_progress: Currently in progress
# - bugs: Bug type issues
```

### Field Validation

Validate custom fields before creating/updating (prevents API errors):

```bash
# Validate before creating
track i new -p PROJ -s "Title" --field "Priority=Major" --validate

# Validate only (dry run - doesn't create)
track i new -p PROJ -s "Title" --field "Priority=Invalid" --validate --dry-run
# Error: Invalid value 'Invalid' for field 'Priority'. Valid values: Critical, Major, Normal, Minor

# Validate before updating
track i u PROJ-123 --field "State=Done" --validate
track i u PROJ-123 --field "State=Done" --validate --dry-run
```

### Workflow Hints

The cache includes workflow hints showing valid state transitions for each project. This helps prevent state transition failures due to workflow constraints.

```bash
# View workflow hints in context
track context -p PROJ

# Output includes:
# Workflow Hints:
#   PROJ (Stage):
#     States: Backlog → Develop → Review → Test → Done*
#     Transitions: 10 forward, 4 backward (* = resolved)
```

**JSON structure** (from `track -o json context`):
```json
{
  "workflow_hints": [{
    "project_short_name": "PROJ",
    "state_fields": [{
      "field_name": "Stage",
      "states": [
        {"name": "Backlog", "is_resolved": false, "ordinal": 1},
        {"name": "Develop", "is_resolved": false, "ordinal": 2},
        {"name": "Done", "is_resolved": true, "ordinal": 6}
      ],
      "transitions": [
        {"from": "Backlog", "to": "Develop", "transition_type": "forward"},
        {"from": "Backlog", "to": "Done", "transition_type": "to_resolved"},
        {"from": "Done", "to": "Backlog", "transition_type": "reopen"}
      ]
    }]
  }]
}
```

**Transition types**:
- `forward`: Moving later in workflow (typical progression)
- `backward`: Moving earlier in workflow (rework/rejection)
- `to_resolved`: Moving to a resolved/completed state
- `reopen`: Moving from resolved back to unresolved

**Use cases**:
- Before state transitions, check if the transition is valid
- Prefer `forward` transitions for normal workflow
- Use `to_resolved` transitions for completion
- Warn user about `backward` transitions (may indicate rework)

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

# === CUSTOM FIELD ADMIN (YouTrack only) ===
track field list                   # List field definitions
track field new "Priority" -t enum -p PROJ -v "Low,Medium,High"
track bundle list -t enum          # List bundles
track bundle create "Status" -t state -v "Open,Done" --resolved "Done"

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
9. **Jira limitations**: No project creation, no subtask conversion, no custom field admin
10. **Query syntax**: YouTrack `project: PROJ` vs Jira `project = PROJ`
11. **Confluence IDs**: Numeric page IDs (`123456`) and space IDs (`65957`), not project keys
12. **Custom field admin**: YouTrack only - use `track field new` for convenience command that creates field with values and attaches to project
