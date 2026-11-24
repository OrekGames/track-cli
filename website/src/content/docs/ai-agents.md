---
title: For AI Agents
description: Using track inside AI coding sessions — JSON output, context aggregation, batch operations, and agent skills.
---

`track` is built to be driven by AI coding assistants (Claude Code, Cursor, and
others) as well as humans. This page summarizes the agent-friendly features. For
the exhaustive guide, see
[`docs/agent_guide.md`](https://github.com/OrekGames/track-cli/blob/main/docs/agent_guide.md)
in the repository.

## JSON output

Every command supports machine-readable JSON via `-o json` (or
`--format json`). Use it whenever a program — or an agent — needs to parse the
result:

```bash
track -o json PROJ-123
track -o json i s "project: PROJ #Unresolved" --all
```

## Session briefing

`track prime` is the cheapest way to make an agent accurate. It prints the
project's real state names, required fields, enum values, assignable users,
tags and query templates in a few hundred tokens, entirely from the local
cache:

```bash
track prime                # Compact briefing: states, required fields, values, users, templates
track prime -p PROJ        # Brief on another cached project
track prime --max-values 5 # Tighter cap per field (default 12)
track -o json prime        # The same facts as JSON
track prime --hook         # For session hooks: silent unless this directory is a track project
```

Long lists are cut to a cap and state how many entries were left out and
which command lists them. A cache older than a day gets a warning line. Run
`track cache refresh` once to populate the cache.

With `--hook` the command is safe to run at the start of every agent session:
it prints nothing unless the current directory has its own `.track.toml`, and
it never fails.

## Context aggregation

`track context` returns everything an agent needs to reason about a configured
backend (or project) in a single call: projects, custom fields (with enum
values), assignable users, the selected workflow pack (if configured), query
templates (pack queries first, then unshadowed built-ins), workflow hints
(valid state transitions), issue counts, and recently accessed issues.

```bash
track context                  # Aggregated context
track context --project PROJ   # Scope to one project
track -o json context          # JSON for parsing
track context --include-issues # Include unresolved issues
```

The JSON reports how old its data is (`cache_updated_at`,
`cache_age_seconds`, `stale`) and, where the backend can tell, the
`current_user` the token belongs to.

A local cache backs this for fast, low-API-cost lookups:

```bash
track cache refresh --if-stale 1h
track cache status
```

**Read-through lookups.** Commands that need a project's id or schema to build a write take
them from the cache when it holds an answer no older than 24 hours, and write back what they
had to fetch. After one `track cache refresh`, a validated `issue create` is a single request,
and so is `issue start`. A cached "no" is never final: before an unknown field or invalid value
becomes an error, the schema is fetched once more from the tracker. Commands that exist to read
(`project fields`, `project list`, `tags list`, `issue get`) always ask the tracker, and
`project fields` and `tags list` refresh the cache with what they read.

| Control | Effect |
|---------|--------|
| `--no-cache` or `TRACK_NO_CACHE=1` | Answer every lookup from the tracker |
| `TRACK_SCHEMA_TTL=7d` | How long a cached schema or project id is trusted (default `24h`) |
| `TRACK_CACHE_DIR=<path>` | Use this directory as the cache, instead of the project or global one |

**One directory per backend.** The cache lives in `.tracker-cache/<backend>/` (or
`~/.tracker-cli/cache/<backend>/`), so switching backends in one directory no longer makes each
refresh overwrite the other. A cache in the old single-directory layout is moved into place the
first time it is used. Lookups only trust a cache written for the same instance (API endpoint,
plus the repository or project for GitHub and GitLab).

Built-in templates are fitted to each project whose schema is cached (YouTrack and Jira): they
use the project's own state field name, and `high_priority`, `in_progress` and `bugs` are built
from the values the project really has. A template with no fitting value is left out for that
project, and `-T` says so instead of running a query that matches nothing.

## Capability audit

Run `track doctor` at session start when you need to know which backend
operations are trustworthy. It audits the effective backend, or every configured
backend with `--all-backends`, and reports per-check statuses in text or JSON.

```bash
track doctor -o json
track doctor --all-backends -o json
track doctor --all-backends --strict
```

`config test` is only a single connectivity probe. `doctor` checks practical
capabilities such as issue search/read, comments, links, field schema, field
admin, articles, and local write validation. It never mutates remote backends;
`--write-check` validates payloads against fetched schema only.

## Batch operations

Operate on many issues in one command — ideal for agents applying a plan:

```bash
track i u PROJ-1,PROJ-2,PROJ-3 --field "Priority=Major"  # Batch update
track i start PROJ-1,PROJ-2,PROJ-3                        # Batch start
track i done PROJ-1,PROJ-2 --state Done                   # Batch complete
track i del PROJ-1,PROJ-2,PROJ-3                          # Batch delete
```

### Batch inspect before acting

Use `track issue inspect` when an agent needs reliable context for many issues
before making decisions. It is intended for backlog audits, phase summaries,
parent/subtask verification, recent-comment review, and preflight checks before
bulk updates.

```bash
# Capture specific issues with comments and links
track i ix PROJ-1,PROJ-2,PROJ-3 --include comments,links -o json

# Inspect a query-selected backlog slice
track i ix --query "project: PROJ #Unresolved" --all --include comments,links -o json

# Stream issue context generated by another tool
track i ix --ids - --include all --jsonl < ids.txt
```

Unlike a shell loop around `track issue get --full`, `inspect` returns one
stable report with `total`, `succeeded`, `failed`, `issues`, and `errors`.
Individual issue failures do not abort the run unless `--strict` is passed, so
agents can report partial results without losing successful context. Requested
include context is explicit via `--include comments,links,subtasks,history,all`;
unsupported include capabilities are warnings, while real include fetch
failures are per-issue errors.

### Declarative apply

For larger changes, `track apply` runs a declarative plan describing the desired
end state, rather than imperative commands. See the
[agent guide](https://github.com/OrekGames/track-cli/blob/main/docs/agent_guide.md)
for the plan format and examples.

## Avoiding shell-escaping issues

When content is complex (multi-line, special characters), read it from a file or
stdin instead of inlining it:

```bash
track i new -p PROJ -s "Title" --body-file ./body.md   # "-" reads from stdin
track i cmt PROJ-123 --body-file ./comment.md
```

## Agent skills

`track` ships installable agent skills that teach assistants how to use the CLI:

```bash
track init --skills           # Install skills only; no backend connection config change
track init --skills --url ... # Combine with configuration initialization
```

The command installs the same `track` skill reference for these agents:

| Agent | Installed path |
| --- | --- |
| Claude Code | `~/.claude/skills/track/SKILL.md` |
| GitHub Copilot | `~/.copilot/skills/track/SKILL.md` |
| Cursor | `~/.cursor/skills/track/SKILL.md` |
| Gemini CLI | `~/.gemini/skills/track/SKILL.md` |

The installed skill is guidance only. It teaches agents the command surface,
JSON mode, context/cache workflow, backend differences, and batch-operation
patterns; credentials stay in `.track.toml` or environment variables.

## Workflow packs

A `[workflow_pack]` in `.track.toml` defines repo-local query templates
(`ready`, `blocked`, …) that overlay cached built-ins at command time. Pack
queries are not written into `.tracker-cache/`. `-T` / `--template` matches the
pack first; a colliding built-in name is a `workflow validate` warning, not an
error.

```bash
track workflow list       # Offline effective queries
track workflow show       # Selected pack + source (repo / user / explicit)
track workflow validate   # Schema, names, and shadows_builtin warnings
track i s -T ready -p PROJ
```

`track context` includes the pack with unexpanded queries and marks pack
templates as higher precedence. An invalid pack fails pack-consuming commands
(`issue search -T`, `workflow *`) instead of silently falling back to built-ins.
Cached built-ins are backend-scoped: the cache records the backend that wrote
it, and `workflow list` / `-T` resolution only use templates matching the
selected backend — a cache refreshed for another backend is ignored, not
leaked.

## Search projection (`--select`)

`issue search` accepts `--select` in JSON/JSONL mode: comma-separated dotted
paths (`--select id_readable,project.short_name`) emit only those fields per
issue; missing paths are omitted. Text mode and every other command reject the
flag, so a projected, minimal shape is always explicit:

```bash
track i s "project: PROJ #Unresolved" -o json --select id_readable,summary
track i s "project: PROJ #Unresolved" -o jsonl --select id_readable,summary
```

## Workflow hints

State transitions differ per backend. `track context` reports valid transitions
as workflow hints so an agent can move an issue through its lifecycle correctly
without hardcoding state names.
