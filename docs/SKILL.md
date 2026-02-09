---
name: track
description: Reference for the `track` CLI issue tracker tool (YouTrack/Jira). Command aliases, search syntax, workflows, and AI-optimized features.
user-invocable: true
disable-model-invocation: false
---

# Track CLI - Agent Reference

> Quick reference for programmatic issue tracking via the `track` CLI. Supports **YouTrack** and **Jira**.

## Quick Context

| Aspect | Details |
|--------|---------|
| **Binary** | `track` (or `target/release/track` if not installed) |
| **Backends** | YouTrack (default), Jira (`-b jira` or `-b j`) |
| **Output** | Text (default) or JSON (`-o json`) |
| **Config** | `.track.toml` (local), `~/.tracker-cli/.track.toml` (global), env vars, or CLI flags |
| **Cache** | `.tracker-cache.json` - run `track cache refresh` for context |
| **AI Context** | `track context` - aggregated context in single command |

## Configuration

### YouTrack

```bash
track init --url https://youtrack.example.com --token YOUR_TOKEN --project PROJ
# Or env: YOUTRACK_URL, YOUTRACK_TOKEN
# Or .track.toml: backend = "youtrack", url = "...", token = "...", default_project = "PROJ"
```

### Jira

```bash
track init --url https://your-domain.atlassian.net --token API_TOKEN --backend jira --email you@example.com
# Or env: JIRA_URL, JIRA_EMAIL, JIRA_TOKEN
# Or .track.toml: backend = "jira", url = "...", email = "...", token = "..."
```

### Config Management

```bash
track config show              # Show current config
track config test              # Test connection
track config keys              # List all config keys
track config backend jira      # Switch default backend
track config set <key> <value> # Set a value
track config get <key>         # Get a value
```

**Config keys**: `backend`, `url`, `token`, `email`, `default_project`, `youtrack.url`, `youtrack.token`, `jira.url`, `jira.email`, `jira.token`

---

## Command Reference

### Issue Operations

| Operation | Command |
|-----------|---------|
| Get issue | `track PROJ-123` (shortcut for `track issue get`) |
| Get (JSON) | `track -o json PROJ-123` |
| Get (full) | `track PROJ-123 --full` |
| Create | `track i new -p PROJ -s "Summary"` |
| Create (validate) | `track i new -p PROJ -s "Summary" --validate` |
| Update | `track i u PROJ-123 --summary "New" --field "Priority=Major"` |
| Update (validate) | `track i u PROJ-123 --field "State=Done" --validate` |
| Delete | `track i del PROJ-123` |
| Search | `track i s "project: PROJ #Unresolved"` |
| Search (template) | `track i s -T unresolved -p PROJ` |
| Comment | `track i cmt PROJ-123 -m "Text"` |
| List comments | `track i comments PROJ-123` |
| Link | `track i link PROJ-1 PROJ-2 -t depends` |
| Start | `track i start PROJ-123` |
| Complete | `track i done PROJ-123` |

For Jira, add `-b j` before the subcommand (e.g., `track -b j PROJ-123`).

### Project Operations

| Operation | Command |
|-----------|---------|
| List | `track p ls` |
| Get | `track p g PROJ` |
| Fields | `track p f PROJ` |
| Create | `track p new -n "Name" -s "KEY"` (YouTrack only) |
| Attach field | `track p attach-field PROJ -f <field-id> --bundle <bundle-id>` (YouTrack only) |

### Custom Field Admin (YouTrack Only)

| Operation | Command |
|-----------|---------|
| List fields | `track field ls` |
| Create field | `track field create "Name" -t enum` |
| Create + attach | `track field new "Name" -t enum -p PROJ -v "Val1,Val2,Val3"` |
| List bundles | `track bundle ls -t enum` |
| Create bundle | `track bundle create "Name" -t enum -v "Val1,Val2"` |
| Add value | `track bundle add-value <id> -t enum -v "NewValue"` |

**Field types**: `enum`, `multi-enum`, `state`, `text`, `date`, `integer`, `float`, `period`

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

**Note**: Confluence uses numeric IDs. YouTrack uses readable IDs (e.g., `KB-A-1`).

### Command Aliases

| Full | Aliases |
|------|---------|
| `track issue` | `track i` |
| `track issue get` | `track i g` |
| `track issue create` | `track i new`, `track i c` |
| `track issue update` | `track i u` |
| `track issue search` | `track i s`, `track i find` |
| `track issue delete` | `track i rm`, `track i del` |
| `track issue comment` | `track i cmt` |
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
| `track bundle list` | `track bundle ls` |

---

## Batch Operations

```bash
# Update multiple issues at once
track i u PROJ-1,PROJ-2,PROJ-3 --field "Priority=Major"

# Start/complete multiple
track i start PROJ-1,PROJ-2,PROJ-3
track i done PROJ-1,PROJ-2 --state Done

# Delete multiple
track i del PROJ-1,PROJ-2,PROJ-3
```

---

## Search Query Syntax

### YouTrack

```bash
track i s "project: PROJ #Unresolved" --limit 20
track i s "project: PROJ State: {In Progress}"
track i s "project: PROJ Assignee: me"
track i s "project: PROJ #Unresolved Priority: Major"
```

### Jira JQL

```bash
track -b j i s "project = PROJ AND resolution IS EMPTY" --limit 20
track -b j i s "project = PROJ AND status = 'In Progress'"
track -b j i s "assignee = currentUser()"
track -b j i s "project = PROJ AND resolution IS EMPTY AND priority = Major"
```

### Syntax Comparison

| Concept | YouTrack | Jira JQL |
|---------|----------|----------|
| Project | `project: PROJ` | `project = PROJ` |
| Unresolved | `#Unresolved` | `resolution IS EMPTY` |
| Resolved | `#Resolved` | `resolution IS NOT EMPTY` |
| Status | `State: {In Progress}` | `status = "In Progress"` |
| Current user | `Assignee: me` | `assignee = currentUser()` |
| Priority | `Priority: Major` | `priority = Major` |
| Text search | `summary:~'keyword'` | `summary ~ "keyword"` |

### Query Templates

Use templates instead of raw queries to avoid syntax errors:

```bash
track i s -T unresolved -p PROJ     # All unresolved issues
track i s -T my_issues -p PROJ      # Assigned to current user
track i s -T recent -p PROJ         # Recently updated (7 days)
track i s -T high_priority -p PROJ  # Critical/Major priority
track i s -T in_progress -p PROJ    # Currently in progress
track i s -T bugs -p PROJ           # Bug type issues
```

---

## Session Startup

```bash
# 1. Verify connection
track config test

# 2. Get aggregated context (recommended)
track context                      # Full context from cache
track context --refresh            # Force refresh from API
track context --include-issues     # Include unresolved issues
track -o json context              # JSON for parsing

# 3. Check cache freshness
track cache status
track cache refresh --if-stale 1h  # Only if older than 1 hour
```

**Duration formats**: `1h`, `30m`, `1d`, `60s`

---

## AI-Optimized Features

### Context Command

```bash
track context                        # Projects, fields, users, templates
track context --project PROJ         # Filter to specific project
track context --refresh              # Force refresh from API
track context --include-issues       # Include unresolved issues
track context --issue-limit 25       # Limit issues (default: 10)
track -o json context                # JSON for parsing
```

**Output includes**: Backend info, projects, custom fields with enum values, tags, link types, query templates, assignable users, workflow hints, recent issues.

### Field Validation

```bash
track i new -p PROJ -s "Title" --field "Priority=Major" --validate
track i new -p PROJ -s "Title" --field "Priority=Invalid" --validate --dry-run
# Error: Invalid value 'Invalid' for field 'Priority'. Valid values: Critical, Major, Normal, Minor
```

### Workflow Hints

```bash
track context -p PROJ
# Shows valid state transitions:
#   PROJ (Stage): Backlog -> Develop -> Review -> Test -> Done*
#   Transitions: 10 forward, 4 backward (* = resolved)
```

**Transition types**: `forward` (normal), `backward` (rework), `to_resolved` (completion), `reopen` (back to unresolved)

---

## Cache System

```bash
track cache refresh       # Fetch all cacheable data
track cache show          # Display cached data
track -o json cache show  # JSON format
track cache path          # Cache file location
track cache status        # Age, freshness, data counts
```

**Cache contents**: Backend metadata, projects, custom fields (with enum values), tags, link types, query templates, project users, recent issues, articles.

---

## Important Notes

1. **Persistent backend**: `track config backend jira` sets default permanently
2. **Backend override**: `-b jira` or `-b j` overrides per-command
3. **JSON output**: Always use `-o json` for programmatic parsing
4. **Issue shortcut**: `track PROJ-123` = `track issue get PROJ-123`
5. **Default project**: `track config set default_project PROJ` to skip `-p` flag
6. **Field discovery**: `track p f PROJ` or `track cache show` lists custom fields with valid values
7. **Cache context**: `track cache refresh` fetches projects, fields, users, link types, query templates, and articles
8. **Query templates**: Cache includes pre-built queries - check `track cache show` for available templates
9. **Jira limitations**: No project creation, no subtask conversion, no custom field admin
10. **Confluence IDs**: Numeric page IDs and space IDs, not project keys
11. **Link types**: `relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, `parent`
12. **Error handling**: Check `track config test` first; common issues are expired tokens and wrong URLs
