---
title: Commands
description: Full command reference and aliases for the track CLI.
---

## Issue shortcuts

```bash
track PROJ-123              # Get issue (shortcut)
track PROJ-123 --full       # With comments, links, subtasks
track open PROJ-123         # Open in browser
```

## Issue commands

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

## Comments

```bash
track issue comment PROJ-123 -m "Comment text"
track issue comments PROJ-123 --limit 10
```

## History

Show an issue's change history — the time-ordered timeline of field
transitions (status changes, assignee changes, etc.) with timestamps and
authors. Supported on all backends.

```bash
track issue history PROJ-123             # Full timeline, newest first
track i hist PROJ-123 --field status     # Only status transitions
track i history PROJ-123 --since 7d      # Last 7 days (s/m/h/d/w)
track -o json i history PROJ-123         # {"issue": id, "changes": [...]}
track -b gh i history 42                 # GitHub (numeric id)
```

`from`/`to` coverage varies by backend: Jira, YouTrack, and Linear carry the
prior value for every field; the event-based backends (GitHub, GitLab) populate
`from` only for `status`.

## Links

```bash
track issue link PROJ-1 PROJ-2              # Relates (default)
track issue link PROJ-1 PROJ-2 -t depends   # Depends on
track issue link PROJ-1 PROJ-2 -t subtask   # Subtask
track issue link PROJ-1 PROJ-2 -t clones    # Custom/admin-defined type

# Unlink (remove a link by its ID — get link IDs from `track i g PROJ-1 --full`)
track issue unlink PROJ-1 "142-3t/PROJ-2"   # YouTrack (composite ID)
track -b j  issue unlink PROJ-1 12345        # Jira (numeric link ID)
track -b gl issue unlink 42 789              # GitLab (numeric link ID)
```

**Built-in link types:** `relates`, `depends`, `required`, `duplicates`,
`duplicated-by`, `subtask`, `parent`. Unrecognized type names are passed
through to the backend as-is, so admin-defined types work without CLI changes.

### Custom link type mappings

Each backend maps canonical link type names to its native name (e.g. `"Blocks"`
on Jira). Override these in your config:

```toml
[jira.link_mappings]
depends = "Requires"
duplicates = "Cloners"

[youtrack.link_mappings]
depends = "Is required for"

[gitlab.link_mappings]
depends = "is_blocked_by"

[linear.link_mappings]
relates = "similar"
```

## Projects

```bash
track project list
track project get PROJ
track project fields PROJ               # Custom fields
track project create -n "Name" -s "KEY" # YouTrack only
```

## Tags

```bash
track tags list   # Lists tags/labels for the configured backend
```

GitHub and GitLab use labels instead of tags; the CLI maps labels to the common
`IssueTag` model.

## Custom fields admin (YouTrack only)

```bash
track field list            # List custom field definitions
track field create ...      # Create a custom field
track bundle list           # List bundles
track bundle create ...     # Create a bundle
```

## Articles (Knowledge Base)

YouTrack uses its built-in Knowledge Base; Jira uses Confluence (at the same
domain with a `/wiki` path).

```bash
# YouTrack
track article list --project PROJ
track article create --project PROJ --summary "Title" --content "Body"
track article update KB-A-1 --content-file ./doc.md

# Jira/Confluence (numeric space ID for --project)
track -b j article list --project 65957 --limit 20
track -b j article create --project 65957 --summary "Title" --content "Body"
```

## Config

```bash
track config test            # Test connection
track config show            # Show current config
track config backend jira    # Set default backend
track config project PROJ    # Set default project
track config set <key> <value>
track config get <key>
track config keys            # List available config keys
track config path            # Show config file path
```

## Cache

```bash
track cache refresh                 # Refresh local cache
track cache refresh --if-stale 1h   # Only refresh if older than 1 hour
track cache status                  # Check cache age and freshness
track cache show                    # Show cached data
track cache path                    # Show cache location
```

## Context (AI-optimized)

```bash
track context                  # Aggregated context for AI sessions
track context --project PROJ   # Filter to a specific project
track context --refresh        # Force refresh from API
track context --include-issues # Include unresolved issues
track -o json context          # JSON for parsing
```

## Command aliases

| Full Command          | Aliases                          |
| --------------------- | -------------------------------- |
| `track issue`         | `track i`                        |
| `track issue get`     | `track i g`                      |
| `track issue create`  | `track i new`, `track i c`       |
| `track issue update`  | `track i u`                      |
| `track issue search`  | `track i s`, `track i find`      |
| `track issue delete`  | `track i rm`, `track i del`      |
| `track issue comment` | `track i cmt`                    |
| `track issue history` | `track i history`, `track i hist`|
| `track issue complete`| `track i done`, `track i resolve`|
| `track issue start`   | `track i start`                  |
| `track issue link`    | `track i link`                   |
| `track issue unlink`  | `track i ul`                     |
| `track project`       | `track p`                        |
| `track project list`  | `track p ls`                     |
| `track project fields`| `track p f`                      |
| `track tags`          | `track t`                        |
| `track article`       | `track a`, `track wiki`          |
| `track config`        | `track cfg`                      |
| `track context`       | `track ctx`                      |
| `track field list`    | `track field ls`                 |
| `track field create`  | `track field c`                  |
| `track bundle list`   | `track bundle ls`                |
| `track bundle create` | `track bundle c`                 |

## Output formats

```bash
track PROJ-123              # Text (default)
track -o json PROJ-123      # JSON
track --format json p ls    # JSON
```
