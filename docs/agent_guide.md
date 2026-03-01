# Track CLI - Agent Guide

> **For AI agents**: Quick reference for programmatic issue tracking. Optimized for fast context retrieval and command generation.

## Quick Context

| Aspect | Details |
|--------|---------|
| **Binary** | `track` (or `target/release/track` if not installed) |
| **Backends** | YouTrack (default), Jira (`-b jira`/`-b j`), GitHub (`-b github`/`-b gh`), GitLab (`-b gitlab`/`-b gl`) |
| **Output** | Text (default) or JSON (`-o json`) |
| **Config** | `.track.toml` in project dir, env vars, or CLI flags |
| **Cache** | `.tracker-cache/` - run `track cache refresh` for context |
| **AI Context** | `track context` - aggregated context in single command |

## Backend Comparison

| Feature | YouTrack | Jira | GitHub | GitLab |
|---------|----------|------|--------|--------|
| **Flag** | `-b youtrack` / `-b yt` (default) | `-b jira` / `-b j` | `-b github` / `-b gh` | `-b gitlab` / `-b gl` |
| **Auth** | Bearer token | Basic Auth (email + API token) | Bearer token (PAT) | Private token |
| **Query** | `project: PROJ #Unresolved` | `project = PROJ AND resolution IS EMPTY` (JQL) | `is:open label:bug` (GitHub search) | `state=opened&labels=bug` (filter params) |
| **Knowledge Base** | Yes (`article` commands) | Yes via Confluence (`article` commands) | No | No |
| **Project Creation** | Yes | No (admin only) | No | No |
| **Issue Delete** | Yes | Yes | No (close instead) | Yes |
| **Issue Links** | Yes | Yes | No (use `#number` references) | Yes |
| **Subtasks** | Yes | Yes | No (use task lists) | No (use issue links) |

## Configuration

**Preferred**: Use a `.track.toml` config file. Avoid passing `--url`/`--token` as CLI flags — those are a last resort for one-off commands.

**Priority** (highest wins): CLI flags → env vars → local `.track.toml` → global `~/.tracker-cli/.track.toml`

### Config File (`.track.toml`) — Recommended

Place in your project directory for per-project config, or at `~/.tracker-cli/.track.toml` for global defaults. Created automatically by `track init`.

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
# api_url = "https://api.github.com"  # default, omit unless using GHE
```

**Note**: GitHub requires both `owner` and `repo`. The API URL defaults to `https://api.github.com`. Set `github.api_url` for GitHub Enterprise.

**GitLab**:
```toml
backend = "gitlab"

[gitlab]
token = "glpat-xxxxxxxxxxxx"
url = "https://gitlab.com/api/v4"
project_id = "12345"
# namespace = "your-group"
```

**Note**: GitLab URL should include the `/api/v4` path. The `project_id` can be a numeric ID or a URL-encoded path (e.g., `group%2Fproject`). Issue operations require `project_id` to be set.

### Quick Setup with `track init`

Creates a `.track.toml` in the current directory:

```bash
track init --url https://youtrack.example.com --token YOUR_TOKEN --project PROJ
track init --url https://company.atlassian.net --token API_TOKEN --backend jira --email you@example.com
track init --url https://api.github.com --token ghp_TOKEN --backend github
track init --url https://gitlab.com/api/v4 --token glpat-TOKEN --backend gitlab
```

### Install Agent Skills

```bash
# Install skills only (no config required)
track init --skills

# Combine with config init
track init --url https://youtrack.example.com --token YOUR_TOKEN --skills
```

This installs the `track` skill reference to `~/.claude/skills/track/`, `~/.copilot/skills/track/`, `~/.cursor/skills/track/`, and `~/.gemini/skills/track/`.

### Environment Variables (alternative)

Environment variables override config file values. Use for CI/CD or when you don't want credentials in a file.

```bash
# YouTrack
export YOUTRACK_URL=https://youtrack.example.com
export YOUTRACK_TOKEN=perm:xxx

# Jira
export JIRA_URL=https://your-domain.atlassian.net
export JIRA_EMAIL=you@example.com
export JIRA_TOKEN=your-api-token

# GitHub
export GITHUB_TOKEN=ghp_xxxxxxxxxxxx
export GITHUB_OWNER=your-org
export GITHUB_REPO=your-repo
# export GITHUB_API_URL=https://github.example.com/api/v3  # GitHub Enterprise

# GitLab
export GITLAB_TOKEN=glpat-xxxxxxxxxxxx
export GITLAB_URL=https://gitlab.com/api/v4
export GITLAB_PROJECT_ID=12345
# export GITLAB_NAMESPACE=your-group
```

### Switching Backends

```bash
# Set default backend persistently
track config backend youtrack  # Switch to YouTrack
track config backend jira      # Switch to Jira
track config backend github    # Switch to GitHub
track config backend gitlab    # Switch to GitLab

# Override per-command
track -b jira PROJ-123         # Use Jira for this command only
track -b gh 42                 # Use GitHub for this command only
track -b gl 42                 # Use GitLab for this command only
```

### Test Connection

```bash
track config test              # Uses configured backend
track -b jira config test      # Override to test Jira
track -b gh config test        # Override to test GitHub
track -b gl config test        # Override to test GitLab
```

### Config Management

```bash
# View/modify configuration
track config show              # Show current config
track config keys              # List all available config keys
track config get <key>         # Get a specific value
track config set <key> <value> # Set a specific value

# Examples
track config set backend github
track config set github.owner "myorg"
track config set github.repo "myrepo"
track config set gitlab.project_id "12345"
track config get default_project
```

**Available config keys**: `backend`, `url`, `token`, `email`, `default_project`, `youtrack.url`, `youtrack.token`, `jira.url`, `jira.email`, `jira.token`, `github.token`, `github.owner`, `github.repo`, `github.api_url`, `gitlab.token`, `gitlab.url`, `gitlab.project_id`, `gitlab.namespace`

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
| Search | `track i s "project: PROJ #Unresolved"` | `track -b j i s "project = PROJ"` | `track -b gh i s "is:open label:bug"` | `track -b gl i s "state=opened"` |
| Comment | `track i cmt PROJ-123 -m "Text"` | `track -b j i cmt PROJ-123 -m "Text"` | `track -b gh i cmt PROJ-42 -m "Text"` | `track -b gl i cmt PROJ-42 -m "Text"` |
| Link | `track i link PROJ-1 PROJ-2` | `track -b j i link PROJ-1 PROJ-2` | Not supported | `track -b gl i link PROJ-1 PROJ-2` |

**GitHub/GitLab notes**:
- GitHub and GitLab use numeric issue IDs (e.g., `42`), not project-prefixed keys
- GitHub does not support deleting issues -- close them with `track -b gh i u PROJ-42 --state closed`
- GitHub does not support issue links -- reference issues via `#42` in comments
- GitHub project (`-p`) is implicit from the configured `owner/repo`

### Project Operations

| Operation | YouTrack | Jira | GitHub | GitLab |
|-----------|----------|------|--------|--------|
| List | `track p ls` | `track -b j p ls` | `track -b gh p ls` | `track -b gl p ls` |
| Get | `track p g PROJ` | `track -b j p g PROJ` | `track -b gh p g owner/repo` | `track -b gl p g 12345` |
| Fields | `track p f PROJ` | `track -b j p f PROJ` | `track -b gh p f owner/repo` | `track -b gl p f 12345` |
| Create | `track p new -n "Name" -s "KEY"` | Not supported | Not supported | Not supported |
| Attach field | `track p attach-field PROJ -f <field-id> --bundle <bundle-id>` | Not supported | Not supported | Not supported |

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

**Note**: Knowledge base is only available for YouTrack and Jira/Confluence. GitHub and GitLab do not support articles. Confluence uses numeric IDs for both pages and spaces. YouTrack uses readable IDs (e.g., `KB-A-1`).

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
 3 issues updated
   PROJ-1
   PROJ-2
   PROJ-3
```

Partial failures show both:
```
 2/3 issues updated (1 failed)
   PROJ-1
   PROJ-2 - Invalid value for field 'State'
   PROJ-3
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

### GitHub Search Examples

```bash
# Open issues (repo is implicit from config)
track -b gh i s "is:open" --limit 20

# By label
track -b gh i s "is:open label:bug"

# By assignee
track -b gh i s "is:open assignee:username"

# Text search
track -b gh i s "is:open memory leak"

# Combined
track -b gh i s "is:open label:bug label:critical assignee:username"
```

### GitLab Filter Examples

```bash
# Open issues (project is implicit from config)
track -b gl i s "state=opened" --limit 20

# By label
track -b gl i s "labels=bug"

# By assignee
track -b gl i s "assignee_username=username"

# Text search
track -b gl i s "search=memory leak"

# Combined (multiple filters)
track -b gl i s "state=opened&labels=bug,critical"
```

### Query Syntax Comparison

| Concept | YouTrack | Jira JQL | GitHub Search | GitLab Filters |
|---------|----------|----------|---------------|----------------|
| Project filter | `project: PROJ` | `project = PROJ` | implicit (owner/repo) | implicit (project_id) |
| Unresolved | `#Unresolved` | `resolution IS EMPTY` | `is:open` | `state=opened` |
| Resolved | `#Resolved` | `resolution IS NOT EMPTY` | `is:closed` | `state=closed` |
| Open status | `State: Open` | `status = Open` | `is:open` | `state=opened` |
| In progress | `State: {In Progress}` | `status = "In Progress"` | `label:in-progress` | `labels=in-progress` |
| Current user | `Assignee: me` | `assignee = currentUser()` | `assignee:@me` | `assignee_username=<user>` |
| Priority | `Priority: Major` | `priority = Major` | `label:priority-major` | `labels=priority::major` |
| Text search | `summary:~'keyword'` | `summary ~ "keyword"` | `keyword` (in query) | `search=keyword` |
| By label | `tag: {bug}` | `labels = bug` | `label:bug` | `labels=bug` |
| AND | implicit or `AND` | `AND` | space-separated | `&`-separated params |
| OR | `OR` | `OR` | N/A | N/A |

---

## Pagination

By default, search and list commands return up to 20 results. Use `--limit` / `--skip` for manual paging, or `--all` to fetch every result automatically.

### Flags

| Flag | Applies to | Behavior |
|------|-----------|----------|
| `--limit N` | `issue search`, `article list`, `article search` | Return at most N results (default: 20) |
| `--skip N` | Same as above | Skip the first N results |
| `--all` | `issue search`, `issue comments`, `article list`, `article search`, `article comments` | Fetch all pages automatically (conflicts with `--limit` / `--skip`) |

### Safety limit

`--all` caps results at **1000** by default to prevent accidental massive fetches. Override with the `TRACK_MAX_RESULTS` environment variable:

```bash
TRACK_MAX_RESULTS=5000 track i s "project: PROJ" --all
```

### Pagination hints

When a search fills its page limit, text output prints a hint to stderr with the total count (if known) and the next `--skip` value:

```
  ┄┄ 20 results shown (20 of 847 total)  ·  use --all or --skip 20 for next page
```

In JSON mode, the `SearchResult` wrapper includes `total` when the backend reports it (Jira, GitHub, GitLab natively; YouTrack via a count API call).

### Examples

```bash
# Fetch first 20 (default)
track i s "project: PROJ #Unresolved"

# Fetch next page
track i s "project: PROJ #Unresolved" --limit 20 --skip 20

# Fetch ALL unresolved issues
track i s "project: PROJ #Unresolved" --all

# Fetch all issues as JSON (for scripting)
track -o json i s "project: PROJ #Unresolved" --all

# Fetch all articles in a project
track article list -p PROJ --all
```

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

# GitHub
track -b gh PROJ-42
track -b gh PROJ-42 --full
track -b gh -o json PROJ-42

# GitLab
track -b gl PROJ-42
track -b gl PROJ-42 --full
track -b gl -o json PROJ-42
```

### Create Issue

```bash
# YouTrack
track i new -p PROJ -s "Bug title" -d "Description" --priority "Major"
track i new -s "Subtask" --parent PROJ-100   # Subtask

# Jira
track -b j i new -p PROJ -s "Bug title" -d "Description"
track -b j i new -s "Subtask" --parent PROJ-100

# GitHub (project implicit from owner/repo config)
track -b gh i new -s "Bug title" -d "Description"

# GitLab (project implicit from project_id config)
track -b gl i new -s "Bug title" -d "Description"
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

# GitHub
track -b gh i u PROJ-42 --summary "New title"
track -b gh i u PROJ-42 --state closed        # Close issue (no delete)

# GitLab
track -b gl i u PROJ-42 --summary "New title"
track -b gl i u PROJ-42 --description "Updated description"
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

# GitHub - Use state field (open/closed only)
track -b gh i u PROJ-42 --state closed

# GitLab - Use state field
track -b gl i u PROJ-42 --state close
```

### Comments

```bash
# Add comment (works with all backends)
track i cmt PROJ-123 -m "Started implementation"
track -b j i cmt PROJ-123 -m "Started implementation"
track -b gh i cmt PROJ-42 -m "Started implementation"
track -b gl i cmt PROJ-42 -m "Started implementation"

# List comments
track i comments PROJ-123
track -b j i comments PROJ-123
track -b gh i comments PROJ-42
track -b gl i comments PROJ-42
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

# GitLab
track -b gl i link PROJ-1 PROJ-2         # relates_to (default)

# GitHub - No native issue links; reference via comments:
track -b gh i cmt PROJ-42 -m "Related to #43"
```

**Link types**: `relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, `parent`

---

## Session Startup

```bash
# 1. Verify connection
track config test                  # YouTrack (default)
track -b jira config test          # Jira
track -b gh config test            # GitHub
track -b gl config test            # GitLab

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
track -o json i s "project: PROJ #Unresolved" --limit 20                          # YouTrack
track -b j -o json i s "project = PROJ AND resolution IS EMPTY" --limit 20        # Jira
track -b gh -o json i s "is:open" --limit 20                                      # GitHub
track -b gl -o json i s "state=opened" --limit 20                                 # GitLab
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

**Output includes**: Backend info, projects, custom fields with enum values, tags/labels, link types, query templates, assignable users, workflow hints (state transitions), issue counts per project/template, recent issues.

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
#     States: Backlog -> Develop -> Review -> Test -> Done*
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

**Cache file** (`.tracker-cache/`) contains:

| Data | Description |
|------|-------------|
| **Backend metadata** | Backend type and base URL |
| **Default project** | From config, for context |
| **Projects** | IDs, short names, descriptions |
| **Custom fields** | Per project: name, type, required flag, **enum values** |
| **Tags/Labels** | Available tags (YouTrack/Jira) or labels (GitHub/GitLab) with IDs and colors |
| **Link types** | Issue link types (Relates, Blocks, Depends, etc.) |
| **Query templates** | Pre-built queries per backend (see below) |
| **Issue counts** | Per project, per template query (e.g., unresolved: 42, bugs: 7) |
| **Project users** | Assignable users per project |
| **Recent issues** | LRU cache of last 50 accessed issues |
| **Articles** | Knowledge base articles with hierarchy (YouTrack/Jira only) |

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
- **GitHub/GitLab**: No knowledge base support -- article commands return errors
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
| `Rate limited` | Too many API requests (GitHub) | Wait and retry; check `x-ratelimit-reset` header |
| `owner not configured` | GitHub missing owner | `track config set github.owner <OWNER>` |
| `repo not configured` | GitHub missing repo | `track config set github.repo <REPO>` |
| `No project ID configured` | GitLab missing project_id | `track config set gitlab.project_id <ID>` |

---

## Quick Reference Card

```bash
# === SETUP & CONTEXT ===
track config test                  # Test connection
track cache refresh                # Refresh cache (recommended first step)
track cache show                   # View cached context (projects, fields, users, query templates, issue counts)

# === BACKEND CONFIGURATION ===
track config backend youtrack      # Set default to YouTrack
track config backend jira          # Set default to Jira
track config backend github        # Set default to GitHub
track config backend gitlab        # Set default to GitLab
track config show                  # Show current config (including backend)
track config keys                  # List all config keys

# === YOUTRACK (when default or with -b yt) ===
track PROJ-123                     # Get issue
track -o json PROJ-123             # Get as JSON
track i s "project: PROJ #Unresolved" --limit 20  # or --all
track i new -p PROJ -s "Summary" --priority "Normal"
track i u PROJ-123 --field "Stage=Done"
track i cmt PROJ-123 -m "Comment"
track i link PROJ-1 PROJ-2 -t depends
track p ls                         # List projects

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

# === GITHUB (when default or with -b gh) ===
track -b gh PROJ-42                # Get issue by number
track -b gh -o json PROJ-42        # Get as JSON
track -b gh i s "is:open" --limit 20
track -b gh i new -s "Summary"     # Create issue (project from config)
track -b gh i u PROJ-42 --summary "New title"
track -b gh i u PROJ-42 --state closed  # Close issue
track -b gh i cmt PROJ-42 -m "Comment"
track -b gh p ls                   # List repos

# === GITLAB (when default or with -b gl) ===
track -b gl PROJ-42                # Get issue by IID
track -b gl -o json PROJ-42        # Get as JSON
track -b gl i s "state=opened" --limit 20
track -b gl i new -s "Summary"     # Create issue (project from config)
track -b gl i u PROJ-42 --summary "New title"
track -b gl i del PROJ-42          # Delete issue
track -b gl i cmt PROJ-42 -m "Comment"
track -b gl i link PROJ-1 PROJ-2   # Link issues
track -b gl p ls                   # List projects
```

---

## Example .track.toml Configurations

### Multi-Backend Setup

```toml
# Default backend
backend = "youtrack"
default_project = "PROJ"

# YouTrack settings
[youtrack]
url = "https://youtrack.example.com"
token = "perm:xxx"

# Jira settings (use with -b jira)
[jira]
url = "https://company.atlassian.net"
email = "user@company.com"
token = "api-token"

# GitHub settings (use with -b github)
[github]
token = "ghp_xxxxxxxxxxxx"
owner = "myorg"
repo = "myrepo"

# GitLab settings (use with -b gitlab)
[gitlab]
token = "glpat-xxxxxxxxxxxx"
url = "https://gitlab.com/api/v4"
project_id = "12345"
```

### GitHub-Only Setup

```toml
backend = "github"

[github]
token = "ghp_xxxxxxxxxxxx"
owner = "myorg"
repo = "myrepo"
```

### GitLab-Only Setup

```toml
backend = "gitlab"

[gitlab]
token = "glpat-xxxxxxxxxxxx"
url = "https://gitlab.com/api/v4"
project_id = "12345"
namespace = "mygroup"
```

---

## Important Notes

1. **Persistent backend**: `track config backend github` sets default permanently
2. **Backend override**: `-b github`/`-b gh` or `-b gitlab`/`-b gl` overrides per-command
3. **JSON output**: Always use `-o json` for programmatic parsing
4. **Issue shortcut**: `track PROJ-123` = `track issue get PROJ-123`
5. **Default project**: `track config project PROJ` to skip `-p` flag
6. **Field discovery**: `track p f PROJ` or `track cache show` lists custom fields with valid values
7. **Cache context**: `track cache refresh` fetches projects, fields, users, link types, query templates, issue counts, and articles
8. **Query templates**: Cache includes pre-built queries - check `track cache show` for available templates
9. **Jira limitations**: No project creation, no subtask conversion, no custom field admin
10. **GitHub limitations**: No issue delete (close instead), no issue links (use `#N` references), no knowledge base
11. **GitLab limitations**: No project creation, no subtask links, no knowledge base
12. **Query syntax**: YouTrack `project: PROJ` vs Jira `project = PROJ` vs GitHub `is:open` vs GitLab `state=opened`
13. **Confluence IDs**: Numeric page IDs (`123456`) and space IDs (`65957`), not project keys
14. **Custom field admin**: YouTrack only - use `track field new` for convenience command that creates field with values and attaches to project
15. **GitHub issue IDs**: Use numeric IDs (e.g., `42`), not project-prefixed keys
16. **GitLab IIDs**: Project-scoped issue numbers; the client strips `#` prefix automatically
17. **Pagination**: Use `--all` to fetch all results; `--limit`/`--skip` for manual paging. Safety cap at 1000 results (override with `TRACK_MAX_RESULTS`)
18. **Issue counts**: `track context` and `track cache show` include per-project issue counts for each query template
