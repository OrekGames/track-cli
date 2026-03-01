---
name: track
description: Reference for the `track` CLI issue tracker tool (YouTrack, Jira, GitHub, GitLab). Command aliases, search syntax, pagination, workflows, and AI-optimized features.
user-invokable: true
disable-model-invocation: false
---

# Track CLI - Agent Reference

> Quick reference for programmatic issue tracking via the `track` CLI. Supports **YouTrack**, **Jira**, **GitHub**, and **GitLab**.

## Quick Context

| Aspect | Details |
|--------|---------|
| **Binary** | `track` (or `target/release/track` if not installed) |
| **Backends** | YouTrack (default), Jira (`-b jira`/`-b j`), GitHub (`-b github`/`-b gh`), GitLab (`-b gitlab`/`-b gl`) |
| **Output** | Text (default) or JSON (`-o json`) |
| **Config** | `.track.toml` (local), `~/.tracker-cli/.track.toml` (global), env vars, or CLI flags |
| **Cache** | `.tracker-cache/` - run `track cache refresh` for context |
| **AI Context** | `track context` - aggregated context in single command |

## Backend Comparison

| Feature | YouTrack | Jira | GitHub | GitLab |
|---------|----------|------|--------|--------|
| **Flag** | `-b youtrack` / `-b yt` (default) | `-b jira` / `-b j` | `-b github` / `-b gh` | `-b gitlab` / `-b gl` |
| **Auth** | Bearer token | Basic Auth (email + API token) | Bearer token (PAT) | Private token |
| **Query** | `project: PROJ #Unresolved` | JQL: `project = PROJ AND resolution IS EMPTY` | `is:open label:bug` | `state=opened&labels=bug` |
| **Knowledge Base** | Yes (`article` commands) | Yes via Confluence | No | No |
| **Project Creation** | Yes | No | No | No |
| **Issue Delete** | Yes | Yes | No (close instead) | Yes |
| **Issue Links** | Yes | Yes | No (use `#number` references) | Yes |

## Configuration

**Preferred**: Use a `.track.toml` config file (created by `track init` or manually). Avoid passing `--url`/`--token` as CLI flags — those are a last resort for one-off commands.

### Config File (`.track.toml`)

Place in your project directory (or `~/.tracker-cli/.track.toml` for global defaults):

**YouTrack**:
```toml
backend = "youtrack"
url = "https://youtrack.example.com"
token = "perm:xxx"
default_project = "PROJ"
```

**Jira**:
```toml
backend = "jira"
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"
```

**GitHub**:
```toml
backend = "github"

[github]
token = "ghp_xxxxxxxxxxxx"
owner = "your-org"
repo = "your-repo"
# api_url = "https://api.github.com"  # default, omit unless GHE
```

**GitLab**:
```toml
backend = "gitlab"

[gitlab]
token = "glpat-xxxxxxxxxxxx"
url = "https://gitlab.com/api/v4"
project_id = "12345"
# namespace = "your-group"
```

### Quick Setup with `track init`

`track init` creates a `.track.toml` in the current directory:

```bash
track init --url https://youtrack.example.com --token YOUR_TOKEN --project PROJ
track init --url https://company.atlassian.net --token API_TOKEN --backend jira --email you@example.com
track init --url https://api.github.com --token ghp_TOKEN --backend github
track init --url https://gitlab.com/api/v4 --token glpat-TOKEN --backend gitlab
```

### Install Agent Skills

```bash
track init --skills    # Install skill files globally for Claude, Copilot, Cursor, and Gemini
```

Can be combined: `track init --url ... --token ... --skills`

### Environment Variables (alternative)

| Backend | Variables |
|---------|-----------|
| YouTrack | `YOUTRACK_URL`, `YOUTRACK_TOKEN` |
| Jira | `JIRA_URL`, `JIRA_EMAIL`, `JIRA_TOKEN` |
| GitHub | `GITHUB_TOKEN`, `GITHUB_OWNER`, `GITHUB_REPO`, `GITHUB_API_URL` |
| GitLab | `GITLAB_TOKEN`, `GITLAB_URL`, `GITLAB_PROJECT_ID`, `GITLAB_NAMESPACE` |

**Priority** (highest wins): CLI flags → env vars → local `.track.toml` → global `~/.tracker-cli/.track.toml`

### Config Management

```bash
track config show              # Show current config
track config test              # Test connection
track config keys              # List all config keys
track config backend github    # Switch default backend
track config set <key> <value> # Set a value
track config get <key>         # Get a value
```

**Config keys**: `backend`, `url`, `token`, `email`, `default_project`, `youtrack.url`, `youtrack.token`, `jira.url`, `jira.email`, `jira.token`, `github.token`, `github.owner`, `github.repo`, `github.api_url`, `gitlab.token`, `gitlab.url`, `gitlab.project_id`, `gitlab.namespace`

---

## Command Reference

### Issue Operations

| Operation | YouTrack | Jira | GitHub | GitLab |
|-----------|----------|------|--------|--------|
| Get issue | `track PROJ-123` | `track -b j PROJ-123` | `track -b gh PROJ-42` | `track -b gl PROJ-42` |
| Get (JSON) | `track -o json PROJ-123` | `track -b j -o json PROJ-123` | `track -b gh -o json PROJ-42` | `track -b gl -o json PROJ-42` |
| Get (full) | `track PROJ-123 --full` | `track -b j PROJ-123 --full` | `track -b gh PROJ-42 --full` | `track -b gl PROJ-42 --full` |
| Create | `track i new -p PROJ -s "Summary"` | `track -b j i new -p PROJ -s "Summary"` | `track -b gh i new -s "Summary"` | `track -b gl i new -s "Summary"` |
| Update | `track i u PROJ-123 --summary "New"` | `track -b j i u PROJ-123 --summary "New"` | `track -b gh i u PROJ-42 --summary "New"` | `track -b gl i u PROJ-42 --summary "New"` |
| Delete | `track i del PROJ-123` | `track -b j i del PROJ-123` | Not supported | `track -b gl i del PROJ-42` |
| Search | `track i s "project: PROJ #Unresolved"` | `track -b j i s "project = PROJ"` | `track -b gh i s "is:open"` | `track -b gl i s "state=opened"` |
| Comment | `track i cmt PROJ-123 -m "Text"` | `track -b j i cmt PROJ-123 -m "Text"` | `track -b gh i cmt PROJ-42 -m "Text"` | `track -b gl i cmt PROJ-42 -m "Text"` |
| Link | `track i link PROJ-1 PROJ-2` | `track -b j i link PROJ-1 PROJ-2` | Not supported | `track -b gl i link PROJ-1 PROJ-2` |
| Start | `track i start PROJ-123` | — | — | — |
| Complete | `track i done PROJ-123` | — | — | — |

**GitHub/GitLab notes**:
- Use numeric issue IDs (e.g., `42`), not project-prefixed keys
- GitHub: no delete (close with `--state closed`), no links (reference via `#42` in comments)
- GitHub/GitLab: project is implicit from config (`owner/repo` or `project_id`)

### Project Operations

| Operation | Command |
|-----------|---------|
| List | `track p ls` (add `-b gh`/`-b gl` for other backends) |
| Get | `track p g PROJ` |
| Fields | `track p f PROJ` |
| Create | `track p new -n "Name" -s "KEY"` (YouTrack only) |

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

**Note**: Knowledge base is YouTrack and Jira/Confluence only. GitHub and GitLab do not support articles. Confluence uses numeric IDs. YouTrack uses readable IDs (e.g., `KB-A-1`).

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

## Pagination

By default, search and list commands return up to 20 results.

| Flag | Behavior |
|------|----------|
| `--limit N` | Return at most N results (default: 20) |
| `--skip N` | Skip the first N results |
| `--all` | Fetch all pages automatically (conflicts with `--limit` / `--skip`) |

**Applies to**: `issue search`, `issue comments`, `article list`, `article search`, `article comments`

**Safety limit**: `--all` caps at **1000** results. Override with `TRACK_MAX_RESULTS` env var.

**Pagination hints**: When a search fills its limit, a hint shows total count and next `--skip` value.

```bash
track i s "project: PROJ #Unresolved"                  # First 20 (default)
track i s "project: PROJ #Unresolved" --limit 20 --skip 20   # Next page
track i s "project: PROJ #Unresolved" --all             # Fetch ALL results
track -o json i s "project: PROJ" --all                 # All results as JSON
track article list -p PROJ --all                        # All articles
TRACK_MAX_RESULTS=5000 track i s "project: PROJ" --all  # Override safety cap
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

### GitHub Search

```bash
track -b gh i s "is:open" --limit 20
track -b gh i s "is:open label:bug"
track -b gh i s "is:open assignee:username"
track -b gh i s "is:open memory leak"
```

### GitLab Filters

```bash
track -b gl i s "state=opened" --limit 20
track -b gl i s "labels=bug"
track -b gl i s "assignee_username=username"
track -b gl i s "state=opened&labels=bug,critical"
```

### Syntax Comparison

| Concept | YouTrack | Jira JQL | GitHub Search | GitLab Filters |
|---------|----------|----------|---------------|----------------|
| Project | `project: PROJ` | `project = PROJ` | implicit (owner/repo) | implicit (project_id) |
| Unresolved | `#Unresolved` | `resolution IS EMPTY` | `is:open` | `state=opened` |
| Resolved | `#Resolved` | `resolution IS NOT EMPTY` | `is:closed` | `state=closed` |
| Status | `State: {In Progress}` | `status = "In Progress"` | `label:in-progress` | `labels=in-progress` |
| Current user | `Assignee: me` | `assignee = currentUser()` | `assignee:@me` | `assignee_username=<user>` |
| Priority | `Priority: Major` | `priority = Major` | `label:priority-major` | `labels=priority::major` |
| Text search | `summary:~'keyword'` | `summary ~ "keyword"` | `keyword` (in query) | `search=keyword` |

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
track config test                  # Uses configured backend
track -b gh config test            # Override to test specific backend

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

**Output includes**: Backend info, projects, custom fields with enum values, tags/labels, link types, query templates, assignable users, workflow hints, issue counts, recent issues.

### Field Validation

`--validate` checks field values against the project schema before submitting. The CLI fetches the schema automatically — **no need to run `project fields` first**.

```bash
# Validate and create (CLI checks schema, then creates)
track i new -p PROJ -s "Title" --field "Priority=High" --field "Type=Bug" --validate

# Validate and update
track i u PROJ-123 --field "Priority=Critical" --validate

# Dry run: validate only, do not submit (shows what would be sent)
track i new -p PROJ -s "Title" --field "Priority=Invalid" --validate --dry-run
# Error: Invalid value 'Invalid' for field 'Priority'. Valid values: Critical, Major, Normal, Minor
```

**Key point**: `--validate` alone still creates/updates if valid. Add `--dry-run` to check without making changes.

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

**Cache contents**: Backend metadata, projects, custom fields (with enum values), tags/labels, link types, query templates, project users, issue counts, recent issues, articles.

---

## Important Notes

1. **Persistent backend**: `track config backend github` sets default permanently
2. **Backend override**: `-b github`/`-b gh` or `-b gitlab`/`-b gl` overrides per-command
3. **JSON output**: Always use `-o json` for programmatic parsing
4. **Issue shortcut**: `track PROJ-123` = `track issue get PROJ-123`
5. **Default project**: `track config set default_project PROJ` to skip `-p` flag
6. **Field discovery**: `track p f PROJ` or `track cache show` lists custom fields with valid values
7. **Cache context**: `track cache refresh` fetches projects, fields, users, link types, query templates, issue counts, and articles
8. **Query templates**: Cache includes pre-built queries - check `track cache show` for available templates
9. **Pagination**: Use `--all` to fetch all results; `--limit`/`--skip` for manual paging. Safety cap at 1000 (override with `TRACK_MAX_RESULTS`)
10. **GitHub limitations**: No issue delete (close instead), no issue links (use `#N` references), no knowledge base
11. **GitLab limitations**: No project creation, no subtask links, no knowledge base
12. **Jira limitations**: No project creation, no subtask conversion, no custom field admin
13. **Confluence IDs**: Numeric page IDs and space IDs, not project keys
14. **GitHub issue IDs**: Use numeric IDs (e.g., `42`), not project-prefixed keys
15. **GitLab IIDs**: Project-scoped issue numbers; the client strips `#` prefix automatically
16. **Link types**: `relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, `parent`
17. **Error handling**: Check `track config test` first; common issues are expired tokens and wrong URLs
18. **Agent skills**: `track init --skills` installs this skill file globally for Claude, Copilot, Cursor, and Gemini
