---
name: track
description: Reference for the `track` CLI issue tracker tool (YouTrack, Jira, GitHub, GitLab, Linear). Command aliases, search syntax, pagination, JSON output shapes, workflows, and AI-optimized features.
user-invokable: true
disable-model-invocation: false
---

# Track CLI - Agent Reference

> Quick reference for programmatic issue tracking via the `track` CLI. Supports **YouTrack**, **Jira**, **GitHub**, **GitLab**, and **Linear**.

## Quick Context

| Aspect | Details |
|--------|---------|
| **Binary** | `track` (or `target/release/track` if not installed) |
| **Backends** | YouTrack (default), Jira (`-b jira`/`-b j`), GitHub (`-b github`/`-b gh`), GitLab (`-b gitlab`/`-b gl`), Linear (`-b linear`/`-b lin`) |
| **Output** | Text (default) or JSON (`-o json`) |
| **Config** | `.track.toml` (local), `~/.tracker-cli/.track.toml` (global), env vars, or CLI flags |
| **Cache** | `.tracker-cache/` (project) or `~/.tracker-cli/cache/` (global) - run `track cache refresh` for context |
| **AI Context** | `track context` - aggregated context in single command |

## Exit Codes & Output Streams

- **Exit code**: `0` = success, `1` = any failure. A partially-applied update (some fields ignored by the backend) still exits `0` — with a warning on stderr.
- **stdout** carries only the result (pretty-printed JSON in `-o json` mode).
- **stderr** carries ALL diagnostics as plain text — errors (`Error: ...`), `⚠ Warning:` lines, pagination hints, progress — **even in JSON mode**. Never parse stdout for error objects; check the exit code and read stderr.

## Backend Comparison

| Feature | YouTrack | Jira | GitHub | GitLab | Linear |
|---------|----------|------|--------|--------|--------|
| **Flag** | `-b youtrack` / `-b yt` (default) | `-b jira` / `-b j` | `-b github` / `-b gh` | `-b gitlab` / `-b gl` | `-b linear` / `-b lin` |
| **Auth** | Bearer token | Basic Auth (email + API token) | Bearer token (PAT) | Private token | Personal API key |
| **Query** | `project: PROJ #Unresolved` | JQL: `project = PROJ AND resolution IS EMPTY` | `is:open label:bug` | `state=opened&labels=bug` | `project: ORE #Unresolved` |
| **Issue IDs** | `PROJ-123` | `PROJ-123` | numeric `42` (or `owner/repo#42`) | numeric `42` (or `#42`) | `ORE-123` |
| **Knowledge Base** | Yes (`article` commands) | Yes via Confluence | Yes via repo wiki (no article comments) | Yes via wiki (no article comments) | No |
| **Project Creation** | Yes | No | No | No | No (teams are projects) |
| **Issue Delete** | Yes | Yes | No (close instead) | Yes | Yes |
| **Issue Links** | Yes | Yes | No (use `#number` references) | Yes | Yes |
| **Subtasks** | Yes | Yes | Yes (sub-issues API) | Yes (GraphQL API) | Yes (`parentId`) |

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

**Linear**:
```toml
backend = "linear"
default_project = "ORE" # Linear team key/name/id

[linear]
token = "lin_api_xxxxxxxxxxxx"
url = "https://linear.app/your-workspace" # used by track open
# api_url = "https://api.linear.app/graphql"
# default_linear_project = "Track CLI"
```

### Quick Setup with `track init`

`track init` creates a `.track.toml` in the current directory (or globally with `-g`/`--global`). **GitHub and GitLab require `--project`**:

```bash
track init --url https://youtrack.example.com --token YOUR_TOKEN --project PROJ
track init --url https://company.atlassian.net --token API_TOKEN --backend jira --email you@example.com
track init --url https://api.github.com --token ghp_TOKEN --backend github --project your-org/your-repo
track init --url https://gitlab.com/api/v4 --token glpat-TOKEN --backend gitlab --project 12345
track init --url https://linear.app/your-workspace --token lin_api_TOKEN --backend linear --project ORE
```

Note: `init` does NOT set `default_project` for GitHub/GitLab (scope comes from `owner`/`repo` / `project_id`), but `issue create` still requires `-p` or `default_project` — see Issue Operations notes.

### Install Agent Skills

```bash
track init --skills    # Install skill files globally for Claude, Copilot, Cursor, and Gemini
```

Can be combined: `track init --url ... --token ... --skills`

### Environment Variables (alternative)

| Backend | Variables |
|---------|-----------|
| Any backend | `TRACKER_BACKEND`, `TRACKER_URL`, `TRACKER_TOKEN`, `TRACKER_CONFIG` (alt config path), `TRACKER_<KEY>` (any config key, e.g. `TRACKER_DEFAULT_PROJECT`) |
| YouTrack | `YOUTRACK_URL`, `YOUTRACK_TOKEN` |
| Jira | `JIRA_URL`, `JIRA_EMAIL`, `JIRA_TOKEN` |
| GitHub | `GITHUB_TOKEN`, `GITHUB_OWNER`, `GITHUB_REPO`, `GITHUB_API_URL` |
| GitLab | `GITLAB_TOKEN`, `GITLAB_URL`, `GITLAB_PROJECT_ID`, `GITLAB_NAMESPACE` |
| Linear | `LINEAR_TOKEN`, `LINEAR_API_URL`, `LINEAR_URL`, `LINEAR_DEFAULT_TEAM`, `LINEAR_DEFAULT_PROJECT` |

**Priority** (highest wins): CLI flags → env vars → local `.track.toml` → global `~/.tracker-cli/.track.toml`. Note `TRACKER_URL`/`TRACKER_TOKEN` bind to the global `--url`/`--token` flags, so they get CLI-flag-level priority.

Every command also accepts `--config <PATH>` to point at an alternate TOML file:
```bash
track --config ./ops/.track.jira.toml i s "project = OPS"
```

### Config Management

```bash
track config show              # Show current config
track config test              # Test connection (needs API; everything else below is offline)
track config keys              # List all config keys
track config backend github    # Switch default backend
track config set <key> <value> # Set a value (-g writes the global file)
track config get <key>         # Get a value
track config clear [-g]        # Delete a config file
```

**Config keys** (settable via `config set`): `backend`, `url`, `token`, `email`, `default_project`, `youtrack.url`, `youtrack.token`, `jira.url`, `jira.email`, `jira.token`, `github.token`, `github.owner`, `github.repo`, `github.api_url`, `gitlab.token`, `gitlab.url`, `gitlab.project_id`, `gitlab.namespace`, `linear.token`, `linear.api_url`, `linear.url`, `linear.default_team`, `linear.default_linear_project`

**Notes**:
- `link_mappings` tables (`[youtrack.link_mappings]`, `[jira.link_mappings]`, `[gitlab.link_mappings]`, `[linear.link_mappings]`) are **config-file-only** — `config set/get` rejects them; edit `.track.toml` directly.
- Secret keys are **redacted on read**: `config get token` prints `(set - hidden)`. You can verify a token is set but never read it back.

---

## Command Reference

### Issue Operations

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
| Start | `track i start PROJ-123` | `track -b j i start PROJ-123 --field Status --state "In Progress"` | `track -b gh i start 42` | `track -b gl i start 42` |
| Complete | `track i done PROJ-123` | `track -b j i done PROJ-123 --field Status --state Done` | `track -b gh i done 42` (closes) | `track -b gl i done 42` (closes) |
| History | `track i history PROJ-123` | `track -b j i history PROJ-123 --field status --since 7d` | `track -b gh i history 42` | `track -b gl i history 42` |

**Bare-ID shortcut**: `track PROJ-123` = `track issue get PROJ-123`, but it only fires for IDs shaped `ALPHANUM-DIGITS` with **exactly one dash** (`PROJ-123`, `my2proj-99`). Purely numeric IDs must use `track i g 42`. Global flags (`-b`, `-o`, ...) must come **before** the ID; the only flag recognized after the ID is `--full`.

**GitHub/GitLab notes**:
- Use numeric issue IDs (`42`), never project-prefixed keys — `track -b gh i g PROJ-42` is a hard error. GitHub also accepts `owner/repo#42`; GitLab also accepts `#42`.
- GitHub: no delete (close with `track -b gh i done 42` or `--state closed`), links only for subtask/parent (reference via `#42` in comments for other relationships)
- Search/get/comment scope is implicit from config (`owner/repo` or `project_id`), but **`issue create` still requires `-p` or `default_project`** (which `track init` does not set for these backends)

**`start`/`done` (all backends)**: both are sugar for a state-field update and accept `--field <STATE_FIELD>` and `--state <VALUE>`. Defaults: `start` = `--field Stage --state Develop`, `done` = `--field Stage --state Done`. On projects using a standard `State`/`Status` field, pass it explicitly (e.g. `track i start PROJ-1 --field State --state "In Progress"`). Jira runs a workflow transition; GitHub/GitLab map to open/close. Discover valid fields/values via `track context -p PROJ` workflow hints.

**Linear notes**:
- Use readable identifiers like `ORE-123`; teams are exposed as `track` projects.
- Native Linear Project is set with `--field "Project=Track CLI"` or `linear.default_linear_project`.
- Unknown labels on create/update are rejected; inspect valid values with `track -o json p f ORE`.

### Multi-line Text Input (`--body-file`)

For multi-line Markdown descriptions/comments, use `--body-file <PATH>` (`-` = stdin) instead of fighting shell quoting. Available on issue create/update (sets description; conflicts `-d`/`--json`), issue/article comment (conflicts `-m`), article create/update (conflicts `-c`), project create, and tags create/update (conflicts `-d`):

```bash
track i new -p PROJ -s "Title" --body-file ./desc.md
track i cmt PROJ-1 --body-file - <<'EOF'
Multi-line comment
with **markdown**.
EOF
```

### Raw JSON Payload Mode (issue create/update)

`--json <JSON>` submits a raw payload and **conflicts with every other field flag including `--validate`/`--dry-run`** (no schema validation in this mode):

```bash
track i new --json '{"project":"PROJ","summary":"Title","customFields":[{"name":"Priority","value":{"name":"Major"}},{"$type":"StateIssueCustomField","name":"Stage","value":{"name":"Done"}},{"$type":"SingleUserIssueCustomField","name":"Assignee","value":{"login":"bob"}}],"tags":[{"name":"bug"}],"parent":"PROJ-9"}'
```

`customFields` entries default to SingleEnum unless `$type` is `StateIssueCustomField` or `SingleUserIssueCustomField`. Update payloads must include at least one field.

### Attachments

```bash
track i attachments PROJ-1                                  # List (JSON: id, name, size, mime_type, url, created, author)
track i attach PROJ-1 a.log b.png [--comment "ctx"]         # Upload (--name/--mime-type only valid with a single file)
track i cmt PROJ-1 -m "see log" --attach run.log            # Comment with attachment (errors where unsupported)
track a attachments <ID> && track a attach <ID> <PATHS>...  # Article attachments (--minor-edit is article-only)
```

Note: `--full` issue JSON does **not** include attachments — `track i attachments` is the only way to list them.

### Project Operations

| Operation | Command |
|-----------|---------|
| List | `track p ls` (add `-b gh`/`-b gl` for other backends) |
| Get | `track p g PROJ` |
| Fields | `track -o json p f PROJ` (text mode omits valid values — use JSON, `cache show`, or `context`) |
| Create | `track p new -n "Name" -s "KEY"` (YouTrack only) |
| Attach existing field | `track p attach-field PROJ -f <FIELD_ID> --bundle <BUNDLE_ID> [--required]` (YouTrack; `--bundle` required for enum/state) |

### Tag Management

```bash
track t ls                                                   # List tags
track t create "regression" --tag-color "#d73a4a" -d "desc"  # Create (hex with or without #)
track t update "regression" --new-name "reg" --tag-color d73a4a
track t rm "regression"
```

**Careful**: the tag color flag is `--tag-color`; `--color` is the global terminal-color mode (`auto|always|never`).

### Custom Field Admin (YouTrack Only)

| Operation | Command |
|-----------|---------|
| List fields | `track field ls` |
| Create field | `track field create "Name" -t enum` |
| Create + attach | `track field new "Name" -t enum -p PROJ --values "Val1,Val2,Val3"` |
| List bundles | `track bundle ls -t enum` |
| Create bundle | `track bundle create "Name" -t enum --values "Val1,Val2"` |
| Add value | `track bundle add-value <BUNDLE_ID> -t enum --value "NewValue"` |

**Careful**: values flags are `--values` (field new, bundle create) and `--value` (bundle add-value) — `-v` is the global `--verbose` flag. `field new` requires both `--project` and `--values`.

**Field types**: `enum`, `multi-enum`, `state`, `text`, `date`, `integer`, `float`, `period`

**State fields with resolved markers**:
```bash
track field new "Status" -t state -p PROJ --values "Open,In Progress,Done" --resolved "Done"
track bundle create "Bug Status" -t state --values "Open,Fixed,Closed" --resolved "Fixed,Closed"
```

### Article Operations (Knowledge Base)

| Operation | YouTrack | Jira/Confluence |
|-----------|----------|-----------------|
| Get | `track a g KB-A-1` | `track -b j a g 123456` |
| List | `track a ls --project PROJ` | `track -b j a ls --project 65957` |
| Search | `track a s "query"` | `track -b j a s "query"` |
| Create | `track a new -p PROJ -s "Title" -c "# Content"` | `track -b j a new -p 65957 -s "Title" -c "# Content"` |
| Update | `track a u KB-A-1 --summary "New"` | `track -b j a u 123456 --summary "New"` |
| Delete | `track a del KB-A-1` | `track -b j a del 123456` |
| Tree / Move | `track a tree KB-A-1` / `track a move KB-A-1 --parent KB-A-2` | same |
| Comments | `track a comments KB-A-1` | `track -b j a comments 123456` |
| Add comment | `track a cmt KB-A-1 -m "Text"` | `track -b j a cmt 123456 -m "Text"` |

**Notes**:
- Content is passed with `-c/--content "# Markdown"` or `--body-file page.md` (mutually exclusive); `--parent <ID>` nests; `a move` without `--parent` moves to root.
- **GitHub/GitLab support articles via the repo wiki** (`track -b gh a ls`, etc.) — except article comments, which they reject. Linear has no knowledge base.
- Confluence uses numeric page/space IDs; YouTrack uses readable IDs (e.g., `KB-A-1`).

### Link Type Mappings

Built-in link types: `relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, `parent`. Unrecognized types pass through to the backend as-is (for admin-defined types like `clones`).

Override default canonical-to-native mappings in config (file-only tables — not settable via `config set`):

```toml
[jira.link_mappings]
depends = "Requires"       # Default: "Blocks"

[youtrack.link_mappings]
depends = "Is required for" # Default: "Depend"

[gitlab.link_mappings]
depends = "is_blocked_by"  # Default: "blocks"

[linear.link_mappings]
depends = "blocks"         # Default: "blocks"
```

Default mappings:

| Canonical | YouTrack | Jira | GitLab | Linear |
|-----------|----------|------|--------|--------|
| `relates` | `Relates` | `Relates` | `relates_to` | `related` |
| `depends` | `Depend` | `Blocks` | `blocks` | `blocks` |
| `required` | `Depend` | `Blocks` | `is_blocked_by` | `blocks` |
| `duplicates` | `Duplicate` | `Duplicate` | `relates_to` | `duplicate` |
| `duplicated-by` | `Duplicate` | `Duplicate` | `relates_to` | `duplicate` |

**Finding a LINK_ID for unlink**: `track -o json PROJ-1 --full` → `links[].id`. (There is no `issue links` subcommand, despite what `issue unlink --help` implies.)

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
| `track issue history` | `track i history`, `track i hist` |
| `track issue complete` | `track i done`, `track i resolve` |
| `track issue start` | `track i start` |
| `track issue link` | `track i link` |
| `track issue unlink` | `track i ul` |
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

### Other Commands

```bash
track open PROJ-123        # Open in browser (no ID = dashboard); -o json returns {"success":bool,"url":...}
track completions zsh      # Shell completions (bash|zsh|fish|powershell|elvish); no config/API needed
```

---

## JSON Output Shapes

`-o json` is the mode for programmatic use. Key shapes:

**Issue** (`i g`, each element of `i s`):
```json
{
  "id": "10234",            // backend-internal — do not display
  "id_readable": "PROJ-123", // use THIS in subsequent commands
  "summary": "...", "description": "... or null",
  "project": {"id": "...", "name": "...", "short_name": "..."},
  "custom_fields": [
    {"State":      {"name": "Stage", "value": "Done", "is_resolved": true}},
    {"SingleEnum": {"name": "Priority", "value": "Major"}},
    {"SingleUser": {"name": "Assignee", "login": "...", "display_name": "..."}},
    {"Text":       {"name": "...", "value": "..."}},
    {"MultiEnum":  {"name": "Components", "values": ["Rendering", "Audio"]}},
    {"Unknown":    {"name": "Sprint", "value": [{"id": 12, "name": "Sprint 4", "state": "active"}]}}
  ],
  "tags": [{"id": "...", "name": "..."}],
  "created": "2024-01-15T10:00:00Z", "updated": "...",
  "resolved": "2024-02-01T08:30:00Z"  // or null
}
```

- `custom_fields` entries are **externally tagged** — the variant name (`State`, `SingleEnum`, ...) is the JSON key.
- `custom_fields` is a **best-effort-lossless projection**: every backend surfaces a field as the most specific variant it can (`State`/`SingleEnum`/`SingleUser`/`Text`/`MultiEnum`), and anything it can't classify is preserved verbatim as `{"Unknown": {"name": "...", "value": <raw json>}}`. `value` is omitted only when the value is structurally unretrievable. This is additive — older consumers that read just `name` still work.
- **Jira** surfaces *all* populated fields (system fields like `fixVersions`/`reporter`/`environment` and every custom field), not a hardcoded subset; ADF rich-text custom fields render to plain text. **`Components`** is a `MultiEnum` named `Components` — filter by area server-side with JQL, e.g. `component = "Rendering"`.
- `resolved` is the **resolution timestamp**, not a closed flag: it can be `null` even for Done issues (e.g. a Jira workflow that never sets Resolution). Test closedness via the State field's `is_resolved`.
- **`--full`** wraps the issue in an envelope: `{"issue": {...}, "links": [{"id", "direction", "link_type", "issues": [...]}], "comments": [{"id", "text", "author", "created"}]}`. Attachments are NOT included — use `track i attachments`.
- **`i s -o json`** returns a bare array — no total or pagination metadata (hints go to stderr in text mode only).
- **`i del -o json`** returns `{"success": true, "message": "..."}`.

**History** (`i history`, all backends):
```json
{
  "issue": "PROJ-123",
  "changes": [
    {
      "at": "2026-05-01T12:03:44Z",
      "author": {"login": "712020:...", "name": "Jane Doe"},  // null if unknown
      "field": "status",        // canonical; "status" is normalized across backends
      "from": "In Progress",    // human-readable; null on first set
      "to": "Done"
    }
  ]
}
```

- Ordered **newest-first**. `author` reuses the comment-author shape (`{login, name}`).
- Filter at the source: `--field status` (canonical name) and `--since 7d` (`s`/`m`/`h`/`d`/`w`).
- No batch form — one call per issue. For flow metrics across a board, search for the candidate set first, then call `i history` per issue.
- **`from` coverage varies by backend.** Jira/YouTrack/Linear carry the prior value for every field. GitHub/GitLab are event-based, so `from` is populated only for `status` (derived from the event sequence); other fields report `from: null` with the new value in `to`. Linear label-change history is not yet included.

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

## Declarative Apply Plans

Use `track apply <plan.json>` for multi-step issue workflows that need ordered, dependent operations against one selected backend. Plans are JSON-only and can create issues, update issues, add comments, link issues, and guarded-delete issues.

```bash
track apply plan.json --dry-run
track apply plan.json --validate --resume /tmp/track-apply-state.json
track -o json apply plan.json
track apply delete-plan.json --allow-delete
```

Key rules:
- The plan file lives wherever you pass it. Use `-` to read JSON from stdin.
- `--dry-run` may read/search/validate, but must not create, update, comment, link, or delete.
- `--validate` validates custom fields on create/update. `defaults.validate: true` in the plan has the same effect for those operations.
- `--resume <path>` is the only place apply state is stored. There is no implicit `.tracker-cache/` write. The state file includes a checksum of the raw plan bytes, completed operation indexes, refs, and operation results.
- Local refs are written as `$name`; create operations populate refs with the created or dedupe-reused readable issue ID, falling back to the backend ID.
- Real `delete_issue` operations require `--allow-delete`; dry-run can inspect delete plans without it. GitHub cannot delete issues, so use close/update behavior there instead.

Minimal plan:

```json
{
  "version": 1,
  "defaults": {"project": "PROJ", "validate": true},
  "operations": [
    {
      "ref": "parent",
      "op": "create_issue",
      "summary": "Parent issue",
      "description": "Created by an apply plan",
      "fields": {"Priority": "Major", "Platform": ["macOS", "Linux"]},
      "tags": ["agent-workflow"],
      "dedupe": {
        "query": "project: PROJ summary: {Parent issue}",
        "on_match": "reuse"
      }
    },
    {
      "op": "update_issue",
      "issue": "$parent",
      "fields": {"State": "In Progress"}
    },
    {
      "op": "comment",
      "issue": "$parent",
      "body": "Created from an apply plan."
    }
  ]
}
```

JSON output includes `success`, `dry_run`, `resumed`, summary counts, `refs`, and per-operation results with `index`, `op`, `status`, `issue`, `ref`, `error`, and `warnings`. Stop on the first failed operation; resume skips completed operations when the checksum still matches.

---

## Pagination

| Flag | Behavior |
|------|----------|
| `--limit N` | Default **20** for `issue search` / `article list` / `article search`; default **10** for `issue comments` / `article comments` |
| `--skip N` | Skip the first N results — only on `issue search`, `article list`, `article search` (NOT comments commands) |
| `--all` | Fetch all pages automatically (conflicts with `--limit` / `--skip`) |

**Safety limit**: `--all` caps at **1000** results. Override with `TRACK_MAX_RESULTS` env var.

**Pagination hints** (text mode only): when a search fills its limit, a hint with the next `--skip` value — and a total when the backend provides one — prints to **stderr**. In `-o json` mode there is no hint and no metadata: treat a full page (`len == limit`) as "possibly more" and re-query with `--skip` or use `--all`.

**Jira specifics**: Jira Cloud search is cursor-based — there is **never a total count**, and `--skip` is emulated client-side by walking pages from the start (deep paging is O(skip)). Prefer `--all` or a narrower JQL over deep `--skip`. Repeating/looping server pages raise a `PaginationStalled` error rather than hanging.

```bash
track i s "project: PROJ #Unresolved"                  # First 20 (default)
track i s "project: PROJ #Unresolved" --limit 20 --skip 20   # Next page
track i s "project: PROJ #Unresolved" --all             # Fetch ALL results
track -o json i s "project: PROJ" --all                 # All results as JSON
track i comments PROJ-1 --all                           # All comments (default is only 10!)
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
track -b j i s "project = PROJ AND statusCategory != Done ORDER BY created DESC"
```

**Passthrough rule**: a query containing `=`, ` AND `, or ` OR ` is sent to Jira **verbatim**, so the full JQL surface evaluates server-side (`ORDER BY`, `statusCategory`, `created >= -90d`, `status CHANGED`, sprint functions, ...). Queries without those tokens are auto-converted from YouTrack-style syntax (`project: PROJ #Unresolved` works on Jira too; leftover words become `text ~ "..."`).

**Traps**:
- A sort-only query like `"ORDER BY created DESC"` contains no `=`/`AND`/`OR` and gets mangled into a text search — anchor it: `"project = PROJ ORDER BY created DESC"`.
- `#Unresolved` maps to `resolution IS EMPTY`, which still matches Done issues whose workflow never set a resolution. Use `statusCategory != Done` for true open-ness.

### GitHub Search

```bash
track -b gh i s "is:open" --limit 20
track -b gh i s "is:open label:bug"
track -b gh i s "is:open assignee:username"
track -b gh i s "is:open memory leak"
```

### GitLab Filters

Only the keys `state`, `labels`, and `search` are honored — **anything else (`assignee_username`, `order_by`, ...) is silently dropped**, returning unfiltered results with no error:

```bash
track -b gl i s "state=opened" --limit 20
track -b gl i s "labels=bug"
track -b gl i s "state=opened&labels=bug,critical"
track -b gl i s "search=memory leak"
```

### Linear

```bash
track -b lin i s "project: ORE #Unresolved"
track -b lin i s "team: ORE state: {In Progress}"
track -b lin i s "project: ORE label: Bug assignee: me"
track -b lin i s "project: ORE priority: High"
```

### Syntax Comparison

| Concept | YouTrack | Jira JQL | GitHub Search | GitLab Filters |
|---------|----------|----------|---------------|----------------|
| Project | `project: PROJ` | `project = PROJ` | implicit (owner/repo) | implicit (project_id) |
| Unresolved | `#Unresolved` | `resolution IS EMPTY` | `is:open` | `state=opened` |
| Resolved | `#Resolved` | `resolution IS NOT EMPTY` | `is:closed` | `state=closed` |
| Status | `State: {In Progress}` | `status = "In Progress"` | `label:in-progress` | `labels=in-progress` |
| Current user | `Assignee: me` | `assignee = currentUser()` | `assignee:@me` | not supported in query |
| Priority | `Priority: Major` | `priority = Major` | `label:priority-major` | `labels=priority::major` |
| Text search | `summary: keyword` | `summary ~ "keyword"` | `keyword` (in query) | `search=keyword` |

Jira fragments above must be embedded in a full query containing `=`/`AND`/`OR` (see passthrough rule); standalone fragments get rewritten as text search.

### Query Templates

Templates resolve from the **local cache** — run `track cache refresh` first, or `-T` fails with "Template not found".

```bash
track i s -T unresolved -p PROJ     # All unresolved issues
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

Caveat: the GitLab `my_issues` template relies on `assignee_username`, which the query parser currently drops — it returns all open issues.

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

**Key points**: `--validate` alone still creates/updates if valid (add `--dry-run` to check without changes). `--json` payload mode conflicts with `--validate`/`--dry-run`.

### Post-Action Verification & Diffing

The CLI verifies issue create/update operations to detect "silent failures" where the backend ignores a field change (e.g., read-only fields, invalid transitions).

- **Warnings**: Ignored fields cause a `⚠ Warning:` on `stderr` (exit code stays 0) — in both text and JSON modes. Always read stderr to see if an update applied fully.
- **Verbose Diffing**: `--verbose` (`-v`) on create/update prints an explicit diff — requested changes (`Old -> New`), server-side side effects (`(Side Effect) Stage: Open -> Done`), ignored requests (`Field (Ignored)`). **Text mode only**: with `-o json` the diff is suppressed (warnings still reach stderr).

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
track cache path          # Cache directory location
track cache status        # Age, freshness, data counts
```

**Cache location**: `.tracker-cache/` in project directory (when `.track.toml` exists), or `~/.tracker-cli/cache/` globally.

**Project context**: When `default_project` is set, cache refresh only fetches detailed data (fields, users, workflows) for that project. Instance-level data (tags, link types, query templates) is always fetched.

**Cache contents**: Backend metadata, projects, custom fields (with enum values), tags/labels, link types, query templates, project users, issue counts, recent issues, articles.

---

## Mock Mode & Eval

Setting `TRACK_MOCK_DIR=./fixtures/scenarios/<name>` routes **every** command to an offline mock backend — no credentials needed, config validation skipped. Useful for rehearsing workflows safely; also a trap: if commands return suspiciously canned data, check that `TRACK_MOCK_DIR` is unset. `track eval status` shows mock state; the `track eval` family (list/show/run/run-all/clear/clear-all/status) scores agent call logs against scenarios and never touches the network.

---

## Important Notes

1. **Persistent backend**: `track config backend github` sets default permanently
2. **Backend override**: `-b github`/`-b gh`, `-b gitlab`/`-b gl`, or `-b linear`/`-b lin` overrides per-command
3. **JSON output**: Always use `-o json` for programmatic parsing — but remember diagnostics stay on stderr, verbose diffs are text-only, and search JSON has no pagination metadata
4. **Issue shortcut**: `track PROJ-123` = `track issue get PROJ-123` (one-dash IDs only; flags go before the ID; only `--full` works after it)
5. **Default project**: `track config set default_project PROJ` to skip `-p` flag
6. **Field discovery**: `track -o json p f PROJ`, `track cache show`, or `track context -p PROJ` list custom fields with valid values (plain `track p f` text output omits the values)
7. **Cache context**: `track cache refresh` fetches projects, fields, users, link types, query templates, issue counts, and articles — and is required before `-T` templates work
8. **Pagination**: `--all` caps at 1000 (override `TRACK_MAX_RESULTS`); comments commands default to `--limit 10` and have no `--skip`
9. **GitHub limitations**: No issue delete (close instead), no general issue links (only subtask/parent; use `#N` references); articles work via repo wiki (no article comments)
10. **GitLab limitations**: No project creation; query filters limited to `state`/`labels`/`search`; articles work via wiki (no article comments)
11. **Linear limitations**: No team creation, no knowledge base; native Linear Project is a field/association, not the CLI project
12. **Jira limitations**: No project creation, no custom field admin, no total counts from search
13. **Confluence IDs**: Numeric page IDs and space IDs, not project keys
14. **GitHub/GitLab issue IDs**: numeric only (`42`, `#42`, `owner/repo#42`) — project-prefixed keys are rejected
15. **Link types**: `relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, `parent` (unrecognized types pass through as-is); get link IDs for `unlink` from `track -o json <ID> --full`
16. **Link type mappings**: Override via `[backend.link_mappings]` tables in `.track.toml` (file-only; not settable via `config set`)
17. **Resolution vs closed**: the JSON `resolved` field is a timestamp that can be null on Done issues — use the State field's `is_resolved` for closedness
18. **Error handling**: exit code 1 + `Error:` on stderr (text even in JSON mode); check `track config test` first; common issues are expired tokens and wrong URLs
19. **Mock mode**: `TRACK_MOCK_DIR` silently redirects all commands to fixtures — verify it is unset when results look canned
20. **Agent skills**: `track init --skills` installs this skill file globally for Claude, Copilot, Cursor, and Gemini
