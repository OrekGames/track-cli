# Query syntax across backends and `-T` templates

Read when translating a search between backends, or using `-T` templates and workflow packs.

## Syntax Comparison

| Concept | YouTrack | Jira JQL | GitHub Search | GitLab Filters |
|---------|----------|----------|---------------|----------------|
| Project | `project: PROJ` | `project = PROJ` | implicit (owner/repo) | implicit (project_id) |
| Unresolved | `#Unresolved` | `resolution IS EMPTY` | `is:open` | `state=opened` |
| Resolved | `#Resolved` | `resolution IS NOT EMPTY` | `is:closed` | `state=closed` |
| Status | `State: {In Progress}` | `status = "In Progress"` | `label:in-progress` | `labels=in-progress` |
| Current user | `Assignee: me` | `assignee = currentUser()` | `assignee:@me` | `assignee_username=@me` |
| Priority | `Priority: Major` | `priority = Major` | `label:priority-major` | `labels=priority::major` |
| Text search | `summary: keyword` | `summary ~ "keyword"` | `keyword` (in query) | `search=keyword` |

Jira fragments above must be embedded in a full query containing `=`/`AND`/`OR` (see passthrough rule); standalone fragments get rewritten as text search.

## Query Templates

Built-in templates are fitted to each project whose schema is cached (YouTrack and Jira): they
use the project's own state field name, and `high_priority`, `in_progress` and `bugs` are built
from the values the project really has. A template with no fitting value is left out for that
project, and `-T` says so instead of running a query that matches nothing.

`-T` / `--template` matches a `[workflow_pack]` query in config first (case-insensitive), then cached built-ins. Pack queries are not written into `.tracker-cache/`. Built-ins still resolve from the **local cache** — run `track cache refresh` if no pack defines the name, or `-T` fails with "Template not found". Cached built-ins are backend-scoped: the cache records the backend that wrote it, and `workflow list` / `-T` resolution only use templates matching the selected backend; a cache refreshed for another backend is ignored, not leaked.

```toml
[workflow_pack]
name = "Orek backlog"
default_project = "PROJ"

[[workflow_pack.queries]]
name = "ready"
description = "Issues ready for implementation."
query = "project: {PROJECT} #Unresolved State: {Ready}"
```

```bash
track workflow list                 # Pack first, then unshadowed built-ins (offline)
track workflow show                 # Selected pack + source
track workflow validate             # Offline; shadows_builtin is a warning
track i s -T ready -p PROJ          # Pack query when defined
track i s -T unresolved -p PROJ     # All unresolved issues (or pack override)
track i s -T my_issues -p PROJ      # Assigned to current user
track i s -T recent -p PROJ         # Recently updated
track i s -T high_priority -p PROJ  # High-priority issues
track i s -T in_progress -p PROJ    # Currently in progress
track i s -T bugs -p PROJ           # Bug type issues
```

Availability and meaning vary by backend:

| Template | YouTrack | Jira | GitHub | GitLab | Linear |
|----------|----------|------|--------|--------|--------|
| `unresolved`, `my_issues`, `bugs` | ✓ | ✓ | ✓ | ✓ | ✓ |
| `recent` | ✓ (7 days) | ✓ (7 days) | ✓ (sort only) | ✓ (sort only) | — |
| `high_priority` | ✓ (Critical/Major) | ✓ (Highest/High) | — | ✓ (`priority::high` label) | ✓ (High) |
| `in_progress` | ✓ | ✓ | — | — | ✓ |
| GitHub extras | | | `enhancements`, `no_assignee` | | |

**Pack validation & precedence**: one pack is selected whole — `--config` / `TRACKER_CONFIG`, else `./.track.toml`, else `~/.tracker-cli/.track.toml`; a project pack shadows the global one. Query names must be lowercase snake_case (`^[a-z][a-z0-9_]*$`), unique within the pack (case-insensitive), with non-empty name/description/query; semantic errors (empty name, duplicates, unknown keys) fail `-T` search and `workflow` commands with a nonzero exit instead of silently falling back to built-ins. `shadows_builtin` is a warning, not an error. Cache behavior: pack queries are never written into `.tracker-cache/`; cached built-ins are backend-scoped (see above).
