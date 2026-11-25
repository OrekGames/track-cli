# Cache, context and mock mode

Read to understand what is cached, how lookups read through it, and what `track context` returns beyond `track prime`.

## Cache System

```bash
track cache refresh       # Fetch all cacheable data
track cache show          # Display cached data
track -o json cache show  # JSON format
track cache path          # Cache directory location
track cache status        # Age, freshness, data counts
```

**Cache location**: `.tracker-cache/<backend>/` in the project directory (when `.track.toml` exists), or `~/.tracker-cli/cache/<backend>/` globally. `TRACK_CACHE_DIR` overrides both.

**Read-through lookups.** Commands that need a project's id or schema to build a write take
them from the cache when it holds an answer no older than 24 hours, and write back what they
had to fetch. After one `track cache refresh`, a validated `issue create` is a single request,
and so is `issue start`. A cached "no" is never final: before an unknown field or invalid value
becomes an error, the schema is fetched once more from the tracker, and a cached project id the
tracker turns down (the project was deleted and recreated) is looked up again before the write
fails. Commands that exist to read (`project fields`, `project list`, `tags list`, `issue get`)
always ask the tracker for what they show, and `project fields` and `tags list` refresh the cache
with what they read.

| Control | Effect |
|---------|--------|
| `--no-cache` or `TRACK_NO_CACHE=1` | Answer every lookup from the tracker (`0`, `false`, `off` or empty leave the cache on). Changes a command makes, such as a new tag or project, still reach the cache |
| `TRACK_SCHEMA_TTL=7d` | How long a cached schema or project id is trusted (default `24h`) |
| `TRACK_CACHE_DIR=<path>` | Use this directory as the cache, instead of the project or global one |

**One directory per backend.** The cache lives in `.tracker-cache/<backend>/` (or
`~/.tracker-cli/cache/<backend>/`), so switching backends in one directory no longer makes each
refresh overwrite the other. A cache in the old single-directory layout is moved into place the
first time it is used. Lookups, `track prime`, `track context` and `cache refresh --if-stale`
only trust a cache written for the same instance (API endpoint, plus the repository or project
for GitHub and GitLab).

**Project context**: When `default_project` is set, cache refresh only fetches detailed data (fields, users, workflows) for that project. Instance-level data (tags, link types, query templates) is always fetched.

**Cache contents**: Backend metadata, projects, custom fields (with enum values), tags/labels, link types, query templates, project users, issue counts, recent issues, articles.

## Prime Command

`track prime` prints the facts worth knowing before any write: the state field and its states
(`*` marks resolved ones), fields required on create, every enum value, assignable users, tags
and `-T` templates with counts. It reads only the local cache. Capped lists end with
`… +N more (<command>)`; lines starting with `!` are warnings (no cache, another backend's
cache, or a cache older than a day). `track -o json prime` gives the same as JSON.

## Context Command

```bash
track context                        # Projects, fields, users, templates
track context --project PROJ         # Filter to specific project
track context --refresh              # Force refresh from API
track context --include-issues       # Include unresolved issues
track context --issue-limit 25       # Limit issues (default: 10)
track -o json context                # JSON for parsing
```

**Freshness**: the JSON has `cache_updated_at`, `cache_age_seconds` and `stale` (true after 24 h); `generated_at` is only when the command ran. `current_user` is the token's account when the backend reports it.

**Output includes**: Backend info, projects, custom fields with enum values, tags/labels, link types, optional `workflow_pack`, query templates (pack first, then unshadowed built-ins), assignable users, workflow hints, issue counts (shadowed built-in names dropped), recent issues.

## Workflow Hints

```bash
track context -p PROJ
# Shows valid state transitions:
#   PROJ (Stage): Backlog -> Develop -> Review -> Test -> Done*
```

The hints list each state field's states in workflow order, with resolved ones marked. No tracker exposes its permitted transitions through this CLI; a refused transition comes back as an error.

## Mock Mode & Eval

Setting `TRACK_MOCK_DIR=./fixtures/scenarios/<name>` routes **every** command to an offline mock backend — no credentials needed, config validation skipped. Useful for rehearsing workflows safely; also a trap: if commands return suspiciously canned data, check that `TRACK_MOCK_DIR` is unset. `track eval status` shows mock state; the `track eval` family (list/show/run/run-all/clear/clear-all/status) scores agent call logs against scenarios and never touches the network.
