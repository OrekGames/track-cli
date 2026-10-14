# Issue #294: Workflow and Query Packs

## Problem

Built-in query templates are generic, cache-backed, and selected only from
`TrackerCache.query_templates`. Repositories need local vocabulary such as
`ready`, `blocked`, and `needs_design`, available immediately from
`.track.toml` without replacing or staling the existing cache model.

## Options

### 1. Runtime config overlay over cached built-ins — chosen

- Read one workflow pack from config at command time.
- Prefer its queries over cached built-ins.
- Add the pack to context without persisting it in the cache.

### 2. Materialize packs during cache refresh

This reuses the current lookup directly, but every config edit requires a
refresh and cached definitions can become stale.

### 3. Separate or multiple pack files

This offers better long-term sharing, but introduces imports, file discovery,
and multi-pack selection prematurely.

The runtime overlay is the smallest design that preserves the current
`CachedQueryTemplate` and issue-count system while making config edits
immediately effective.

## Pack format

```toml
[workflow_pack]
name = "Orek backlog"
description = "Project-local views for backlog work."
default_project = "PROJ"

[[workflow_pack.queries]]
name = "ready"
description = "Issues ready for implementation."
query = "project: {PROJECT} #Unresolved State: {Ready}"

[[workflow_pack.queries]]
name = "blocked"
description = "Issues blocked by dependencies or external decisions."
query = "project: {PROJECT} #Unresolved tag: blocked"

[[workflow_pack.queries]]
name = "needs_design"
description = "Issues needing design clarification."
query = "project: {PROJECT} #Unresolved Type: {Design}"
```

Schema:

| Field | Requirement |
| --- | --- |
| `workflow_pack.name` | Required, non-empty human-readable label |
| `workflow_pack.description` | Optional; if present, non-empty |
| `workflow_pack.default_project` | Optional, non-empty project key |
| `workflow_pack.queries` | Required, at least one entry |
| `queries[].name` | Required |
| `queries[].description` | Required, non-empty |
| `queries[].query` | Required, non-empty |

Query-name rules:

- ASCII lowercase snake case: `^[a-z][a-z0-9_]*$`.
- No hyphens; this follows current names such as `my_issues` and
  `high_priority`.
- Names within one pack must be unique case-insensitively.
- CLI lookup remains case-insensitive using `unicode_eq_ignore_case`.
- Built-in names are not reserved because intentional shadowing is supported.
- Only exact `{PROJECT}` is substituted. Backend syntax such as `{Ready}` or
  `{In Progress}` remains untouched.
- Unknown fields inside `workflow_pack` or its query records should be rejected
  to catch configuration typos.

Suggested Rust structures belong in `crates/track/src/config.rs`:

- `Config::workflow_pack: Option<WorkflowPack>`
- `WorkflowPack`
- `WorkflowPackQuery`
- A non-serialized `WorkflowPackSource` (`Repo`, `User`, `Explicit`)

Including the field in `Config` is necessary so `Config::save` and commands
such as `config set` do not silently discard the pack.

## Load path

Pack selection is whole-pack, not field-by-field merging:

1. If `--config PATH` or `TRACKER_CONFIG` is supplied, inspect only that file.
2. Otherwise, use `[workflow_pack]` from `./.track.toml` if present.
3. Otherwise, use `[workflow_pack]` from
   `~/.tracker-cli/.track.toml`.
4. Otherwise, there is no configured pack.

Rules:

- A repo pack completely replaces the user pack; query arrays and metadata are
  never combined.
- Reuse the existing path helpers and explicit-config behavior in
  `config_paths`, `local_track_config_path`, and `global_config_path`.
- Do not search parent directories; current config discovery checks only the
  working directory.
- Complex pack definitions are config-file-only. Do not support
  environment-variable construction or `track config set workflow_pack...`.
- Remaining configuration still follows existing Figment precedence. Only the
  workflow-pack section receives whole-object source selection to avoid a
  hybrid global/local pack.

Built-ins remain backend-specific and continue to use the existing
`CachedQueryTemplate` model. `TrackerCache::get_query_templates` should become
accessible within the crate for collision validation, while on-disk cache
behavior remains unchanged.

## Selection

### Pack selection

There is one effective pack. No pack name or `--pack` argument is selected in
this version; source precedence selects it.

### Query selection

For `track issue search --template`/`-T` and the shared `issue inspect -T`
path:

1. A direct query, when supplied, is used unchanged. Clap already rejects
   supplying both query and template.
2. Validate the effective pack.
3. Match the requested name against effective-pack queries,
   case-insensitively.
4. If absent, fall back to the current backend's built-in templates.
5. If still absent, fail and list available pack and built-in names.

The implementation point is `commands::issue::resolve_search_query`, also
called by `commands::inspect::handle_inspect`.

### Collision policy

Prefer the effective repo or user pack over a built-in template.

Overriding a generic concept such as `unresolved` with repository semantics is
a primary use case. Rejecting all collisions would force awkward names and
prevent deliberate customization.

Collision handling:

- Pack-versus-built-in collision: valid, pack wins, and `workflow validate`
  reports a warning.
- Duplicate names inside one pack: validation error; no arbitrary "first
  wins."
- Repo versus user pack: no collision because repo selects the entire pack.
- An invalid pack causes pack-consuming commands to fail rather than silently
  falling back to built-ins. Unrelated commands such as `issue get` need not
  run semantic pack validation.

### Project expansion

For a selected pack query containing `{PROJECT}`:

1. `--project`/`-p`
2. `workflow_pack.default_project`
3. top-level `Config.default_project`
4. otherwise error

For a built-in query:

1. `--project`/`-p`
2. top-level `Config.default_project`
3. otherwise error

Require a project only when the selected query actually contains `{PROJECT}`.
This removes the current unnecessary project requirement for templates such as
GitLab filters that have no placeholder. Replace every exact occurrence.

### Context

Add an optional `workflow_pack` member to `AggregatedContext` containing:

```json
{
  "source": "repo",
  "name": "Orek backlog",
  "description": "Project-local views for backlog work.",
  "default_project": "PROJ",
  "queries": [
    {
      "name": "ready",
      "description": "Issues ready for implementation.",
      "query": "project: {PROJECT} #Unresolved State: {Ready}"
    }
  ]
}
```

Context behavior:

- Query strings remain unexpanded.
- `context --project` does not select or filter the pack.
- Build `AggregatedContext.query_templates` as a runtime effective view: pack
  queries first, followed by unshadowed cached built-ins.
- Remove cached `issue_counts` whose built-in template name is shadowed,
  because that count would not describe the query selected by `-T`.
- Local non-colliding queries have no count in this slice.
- Do not write pack queries into `.tracker-cache/`.
- Text output shows pack metadata and clearly marks pack queries as higher
  precedence.

## Command surface

### Ships in this slice

#### `track workflow list`

- Offline.
- Lists effective pack queries followed by unshadowed cached built-ins.
- Each JSON item includes `name`, `description`, `query`, `source`, and
  `shadows_builtin`.
- If no cache exists, list pack queries and explain that built-ins require the
  existing cache refresh path.

#### `track workflow show`

- Offline.
- Shows the selected pack, source, metadata, and queries.
- No pack: text explanation or JSON `null`, exit zero.

#### `track workflow validate`

- Offline and must run without valid tracker credentials.
- Checks required fields, name syntax, duplicate names, and collisions with
  compiled built-ins for the effective backend.
- Collision is a warning; semantic errors exit nonzero.
- JSON shape:

  ```json
  {
    "valid": true,
    "source": "repo",
    "errors": [],
    "warnings": [
      {
        "code": "shadows_builtin",
        "query": "unresolved",
        "message": "Repo query shadows the built-in template."
      }
    ]
  }
  ```

- Does not execute queries or contact the backend.

Dispatch workflow commands in `main::run` after config loading and backend
selection but before `Config::validate` and client construction.

## Out of scope

- Shared pack files, imports, exports, URLs, and team-level packs.
- Multiple named packs or `--pack`.
- `terms` and `field_aliases`.
- Executable state-transition workflows or tracker mutations.
- Per-query backend or project scoping.
- Config editing through `track config set`.
- Caching local-query counts.
- Portable or live backend-query syntax validation.
- Variables other than exact `{PROJECT}`.
- Changing the built-in template definitions.
- Selecting a pack for `context --include-issues`; that path keeps its current
  backend-specific unresolved query in this slice.

## Risks

- `.track.toml` can contain tokens, and `track init` currently adds it to
  `.gitignore`. "Repo-defined" in this slice means working-directory scoped,
  not safely committed or team-shared.
- Query syntax is backend-specific; validation cannot prove server acceptance.
- Global user-pack fallback could surprise users, so `workflow show`,
  `workflow list`, and context must expose the source.
- Existing cache files can contain older built-ins after a binary upgrade;
  normal cache refresh behavior remains applicable.

## Open questions

None block this slice. Follow-ups may decide the committed shared-pack
location, backend scoping, and whether local-query counts should enter the
cache.

## Implementer map

Primary files and symbols:

- `crates/track/src/config.rs`
  - `Config`, `Config::load_raw`, `Config::load_from_path`, `config_paths`
  - Add pack schema, whole-pack source selection, and semantic validation.
- `crates/track/src/cache.rs`
  - `CachedQueryTemplate`, `TrackerCache::get_query_templates`,
    `TrackerCache::refresh`
  - Keep persistence and count generation built-in-only.
- `crates/track/src/commands/issue.rs`
  - `handle_issue`, `resolve_search_query`, `try_cached_count`
- `crates/track/src/commands/inspect.rs`
  - Shared template-resolution call.
- `crates/track/src/commands/context.rs`
  - `AggregatedContext`, `handle_context`
- `crates/track/src/cli.rs`
  - `Commands`, new `WorkflowCommands`
- `crates/track/src/main.rs`
  - `run`, `run_with_client`
- `crates/track/src/commands/workflow.rs`
  - New offline list, show, and validate handlers.
- `crates/track/src/commands/mod.rs`
  - Register the command module.

Test locations an implementer should extend:

- Unit tests in `config.rs`, `issue.rs`, and `cli.rs`
- `crates/track/tests/cache_integration_tests.rs` for context output
- A focused `crates/track/tests/workflow_integration_tests.rs` for source
  precedence, collisions, offline commands, and template execution

Agent documentation:

- `docs/agent_guide.md`
- `agent-skills/SKILL.md`
- `website/src/content/docs/ai-agents.md`
- `website/src/content/docs/configuration.md`
- `website/src/content/docs/commands.md`

`docs/agent_guide.md` and `agent-skills/SKILL.md` are embedded by
`commands/init.rs`; no generated copies should be edited.

## Done when

Code Optimizer can implement without choosing schema, source precedence,
collision behavior, project fallback, context representation, cache ownership,
validation severity, command semantics, or scope boundaries.
