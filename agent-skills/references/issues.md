# Issue commands in detail

Read for the less common issue operations: multi-line text, raw JSON payloads, attachments, batches, pagination, validation and verification.

## Issue Operations

| Operation | YouTrack | Jira | GitHub | GitLab |
|-----------|----------|------|--------|--------|
| Get issue | `track PROJ-123` | `track -b j PROJ-123` | `track -b gh i g 42` | `track -b gl i g 42` |
| Get (JSON) | `track -o json PROJ-123` | `track -b j -o json PROJ-123` | `track -b gh -o json i g 42` | `track -b gl -o json i g 42` |
| Get (full) | `track PROJ-123 --full` | `track -b j PROJ-123 --full` | `track -b gh i g 42 --full` | `track -b gl i g 42 --full` |
| Create | `track i new -p PROJ -s "Summary"` | `track -b j i new -p PROJ -s "Summary"` | `track -b gh i new -p owner/repo -s "Summary"` | `track -b gl i new -p 12345 -s "Summary"` |
| Update | `track i u PROJ-123 --summary "New"` | `track -b j i u PROJ-123 --summary "New"` | `track -b gh i u 42 --summary "New"` | `track -b gl i u 42 --summary "New"` |
| Delete | `track i del PROJ-123` | `track -b j i del PROJ-123` | Not supported | `track -b gl i del 42` |
| Search | `track i s "project: PROJ #Unresolved"` | `track -b j i s "project = PROJ"` | `track -b gh i s "is:open"` | `track -b gl i s "state=opened"` |
| Comment | `track i cmt PROJ-123 -m "Text"` | `track -b j i cmt PROJ-123 -m "Text"` | `track -b gh i cmt 42 -m "Text"` | `track -b gl i cmt 42 -m "Text"` |
| Link | `track i link PROJ-1 PROJ-2` | `track -b j i link PROJ-1 PROJ-2` | Subtask/parent only | `track -b gl i link 1 2` |
| Unlink | `track i ul PROJ-1 <link-id>` | `track -b j i ul PROJ-1 <link-id>` | Not supported | `track -b gl i ul 42 <link-id>` |
| Start | `track i start PROJ-123` | `track -b j i start PROJ-123` | `track -b gh i start 42` (reopens) | `track -b gl i start 42` (reopens) |
| Complete | `track i done PROJ-123` | `track -b j i done PROJ-123` | `track -b gh i done 42` (closes) | `track -b gl i done 42` (closes) |
| History | `track i history PROJ-123` | `track -b j i history PROJ-123 --field status --since 7d` | `track -b gh i history 42` | `track -b gl i history 42` |
| Inspect (multi) | `track i ix PROJ-1,PROJ-2 --include comments,links` | `track -b j i ix --query "project = PROJ" --include all` | `track -b gh i ix 42,43 --include comments` | `track -b gl i ix 42,43 --include links` |

**`inspect` (alias `ix`)** fetches many issues in one run with per-issue success/failure. Issue sets: positional comma-separated IDs and/or `--ids <path>` (one per line, `-` = stdin; merged and deduplicated), `--query`, or `--template`+`--project` (with `--limit`/`--skip`/`--all`). Context is opt-in via `--include comments,links,subtasks,history,all` (comma-separated and/or repeated). A failed issue lands in `errors` without aborting; `--strict` exits non-zero after the full report. `--jsonl` streams one result object per line. In query mode the JSON report adds `query_total` (backend-reported match count, when known) alongside `total` (results in this report), so a `--limit`-truncated page is detectable.

**Bare-ID shortcut**: `track PROJ-123` = `track issue get PROJ-123`, but it only fires for IDs shaped `ALPHANUM-DIGITS` with **exactly one dash** (`PROJ-123`, `my2proj-99`). Purely numeric IDs must use `track i g 42`. Global flags (`-b`, `-o`, ...) must come **before** the ID; the only flag recognized after the ID is `--full`.

**GitHub/GitLab notes**:
- Use numeric issue IDs (`42`), never project-prefixed keys — `track -b gh i g PROJ-42` is a hard error. GitHub also accepts `owner/repo#42`; GitLab also accepts `#42`.
- GitHub: no delete (close with `track -b gh i done 42` or `--state closed`), links only for subtask/parent (reference via `#42` in comments for other relationships)
- Search/get/comment scope is implicit from config (`owner/repo` or `project_id`), but **`issue create` still requires `-p` or `default_project`** (which `track init` does not set for these backends)

**`start`/`done` (all backends)**: both are sugar for a state-field update and accept `--field <STATE_FIELD>` and `--state <VALUE>`. The field defaults to the project's state field. The state defaults to the backend's own name: `start` uses `Develop` (YouTrack), `In Progress` (Jira, Linear) or `open` (GitHub, GitLab); `done` uses `Done` (YouTrack, Jira, Linear) or `closed` (GitHub, GitLab). When a workflow names its states differently, pass the state explicitly (e.g. `track i start PROJ-1 --state "In Review"`). Jira runs a workflow transition; Linear falls back to the team's first started/completed state; GitHub/GitLab map to open/close. Discover valid fields/values via `track prime`.

**Linear notes**:
- Use readable identifiers like `ORE-123`; teams are exposed as `track` projects.
- Native Linear Project is set with `--field "Project=Track CLI"` or `linear.default_linear_project`.
- Unknown labels on create/update are rejected; inspect valid values with `track -o json p f ORE`.

## Multi-line Text Input (`--body-file`)

For multi-line Markdown descriptions/comments, use `--body-file <PATH>` (`-` = stdin) instead of fighting shell quoting. Available on issue create/update (sets description; conflicts `-d`/`--json`), issue/article comment (conflicts `-m`), article create/update (conflicts `-c`), project create, and tags create/update (conflicts `-d`):

```bash
track i new -p PROJ -s "Title" --body-file ./desc.md
track i cmt PROJ-1 --body-file - <<'EOF'
Multi-line comment
with **markdown**.
EOF
```

## Raw JSON Payload Mode (issue create/update)

`--json <JSON>` submits a raw payload and **conflicts with every other field flag including `--validate`/`--no-validate`/`--dry-run`** (never validated):

```bash
track i new --json '{"project":"PROJ","summary":"Title","customFields":[{"name":"Priority","value":{"name":"Major"}},{"$type":"StateIssueCustomField","name":"Stage","value":{"name":"Done"}},{"$type":"SingleUserIssueCustomField","name":"Assignee","value":{"login":"bob"}}],"tags":[{"name":"bug"}],"parent":"PROJ-9"}'
```

`customFields` entries default to SingleEnum unless `$type` is `StateIssueCustomField` or `SingleUserIssueCustomField`. Update payloads must include at least one field.

## Attachments

```bash
track i attachments PROJ-1                                  # List (JSON: id, name, size, mime_type, url, created, author)
track i attach PROJ-1 a.log b.png [--comment "ctx"]         # Upload (--name/--mime-type only valid with a single file)
track i cmt PROJ-1 -m "see log" --attach run.log            # Comment with attachment (errors where unsupported)
track a attachments <ID> && track a attach <ID> <PATHS>...  # Article attachments (--minor-edit is article-only)
```

Note: `--full` issue JSON does **not** include attachments — `track i attachments` is the only way to list them.

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

A batch reports every issue's result, then exits `1` if any of them failed.

## Pagination

| Flag | Behavior |
|------|----------|
| `--limit N` | Default **20** for `issue search` / `article list` / `article search`; default **10** for `issue comments` / `article comments` |
| `--skip N` | Skip the first N results — only on `issue search`, `article list`, `article search` (NOT comments commands) |
| `--all` | Fetch all pages automatically (conflicts with `--limit` / `--skip`) |

**Safety limit**: `--all` caps at **1000** results. Override with `TRACK_MAX_RESULTS` env var.

**Pagination hints**: When a search fills its limit, a one-line hint with the next `--skip` value (and a total when one is known) goes to stderr, in JSON and JSONL mode too: a full page of JSON otherwise looks exactly like a complete result. `-o agent` puts the same verdict on the last line of stdout.

**Jira specifics**: Jira Cloud search is cursor-based — there is **never a total count**, and `--skip` is emulated client-side by walking pages from the start (deep paging is O(skip)). Prefer `--all` or a narrower JQL over deep `--skip`. Repeating/looping server pages raise a `PaginationStalled` error rather than hanging.

```bash
track i s "project: PROJ #Unresolved"                  # First 20 (default)
track i s "project: PROJ #Unresolved" --limit 20 --skip 20   # Next page
track i s "project: PROJ #Unresolved" --all             # Fetch ALL results
track -o json i s "project: PROJ" --all                 # All results as JSON
track -o jsonl i s "project: PROJ" --all                # All results, one issue per line
track i comments PROJ-1 --all                           # All comments (default is only 10!)
TRACK_MAX_RESULTS=5000 track i s "project: PROJ" --all  # Override safety cap
track i s "project: PROJ" -o json --select id_readable,summary                # Projected JSON
track i s "project: PROJ" -o jsonl --select id_readable,project.short_name   # JSON Lines
```

## Field Validation

Field updates are checked before they are sent, on every `issue create`, `issue update` and
`apply` operation:

| Mode | Flag | What is rejected |
|------|------|------------------|
| Default | (none) | Only what the backend would reject or silently drop (see below) |
| Strict | `--validate` | Any field name not in the project schema, and any value not in a listed value set. Failing to fetch the schema is an error |
| Off | `--no-validate` | Nothing; fields are sent as given |

What the default mode can reject depends on how much a backend's schema says:

| Backend | Unknown field name | Invalid value |
|---------|--------------------|---------------|
| YouTrack | Error | Error, for every field with a value list |
| GitHub, GitLab | Error. Only the state and `Assignee` are settable with field flags; use `--tag` for labels | Error for the state (anything that is not open/closed or an alias of them) |
| Jira | Warning | Error for `Priority`, `Type` and `Components` when their lists were read from the instance; the status is left to the API |
| Linear | Error. The state, `Priority`, `Assignee`, `Labels` (or `Tags`) and `Project` are settable | Error for the state and `Priority`; assignee, labels and project are left to the API |

An error names the fix: `Unknown field 'Prio'. Did you mean 'Priority'? Available fields: State,
Priority, Type`, or the list of valid values with the nearest one.

The default check never costs an extra request. It uses the schema the command already needs
(any `--field` needs it for typing) or a cached one. With only `--state`, `--priority` or
`--assignee` and no cache, nothing is checked. `--dry-run` no longer needs `--validate`: it
checks, reports, and writes nothing.

`issue start` and `issue complete` check the state before they write. When the project's state
field has no state with the backend's default name, the command fails without touching the
issue and lists the states the project does have (`Invalid value 'Develop' for field 'State'.
Valid values: Open, In Progress, Done. Pass --state <NAME> to choose one.`). `issue complete`
without `--state` uses the project's only resolved state when there is exactly one. An explicit
`--field` skips the check and every lookup.

## Post-Action Verification & Diffing

The CLI verifies issue create/update operations to detect "silent failures" where the backend ignores a field change (e.g., read-only fields, invalid transitions).

- **Warnings**: Ignored fields cause a `⚠ Warning:` on `stderr` (exit code stays 0) — in both text and JSON modes. Always read stderr to see if an update applied fully.
- **Verbose Diffing**: `--verbose` (`-v`) on create/update prints an explicit diff — requested changes (`Old -> New`), server-side side effects (`(Side Effect) Stage: Open -> Done`), ignored requests (`Field (Ignored)`). **Text mode only**: with `-o json` the diff is suppressed (warnings still reach stderr).
