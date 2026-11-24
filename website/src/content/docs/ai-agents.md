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
track prime --hook=claude  # For session hooks (claude, codex, cursor, antigravity, gemini, orek)
```

Long lists are cut to a cap and state how many entries were left out and
which command lists them. A cache older than a day gets a warning line, and so
does one written for another instance (a changed URL, a committed
`.tracker-cache/`), whose data is not shown. Run `track cache refresh` once to
populate the cache.

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
for GitHub and GitLab): `prime` says to refresh any other, `context` fetches afresh, and
`--if-stale` refreshes it however recent it is.

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

## Agent setup

```bash
track agent install                  # For you: every supported tool, in your home directory
track agent install --scope project  # For the team: writes into this repository (commit the result)
track agent install --tools orek,claude   # Only the tools you name
track agent status                   # What is installed, per tool, for you and for this directory
track agent uninstall [--scope project] [--tools ...]
```

Each tool gets what its own documentation says it reads, and nothing it does not:

| Tool | Skill (user / project) | Session-start hook (user / project) | Instructions |
|------|------------------------|-------------------------------------|--------------|
| Orek.ai | Orek's user config `skills/` / `.orek/skills/` | none yet: the hook is ready for when Orek.ai ships session hooks | `AGENTS.md` (project) |
| Claude Code | `~/.claude/skills/` / `.claude/skills/` | `~/.claude/settings.json` / `.claude/settings.json` | `~/.claude/CLAUDE.md`; `@AGENTS.md` import in an existing `CLAUDE.md` |
| Codex | `~/.agents/skills/` / `.agents/skills/` | `$CODEX_HOME/hooks.json` / `.codex/hooks.json` | `$CODEX_HOME/AGENTS.md`; `AGENTS.md` |
| Cursor | `~/.agents/skills/` / `.agents/skills/` | `~/.cursor/hooks.json` / `.cursor/hooks.json` | `AGENTS.md` (project) |
| Antigravity | `~/.gemini/config/skills/` / `.agents/skills/` | `~/.gemini/config/hooks.json` / `.agents/hooks.json` | `~/.gemini/GEMINI.md`; `AGENTS.md` |
| Gemini CLI | `~/.agents/skills/` / `.agents/skills/` | `~/.gemini/settings.json` / `.gemini/settings.json` | `~/.gemini/GEMINI.md`; `@./AGENTS.md` import in an existing `GEMINI.md` |
| Copilot | `~/.copilot/skills/` (user only) | not installed | not installed |

Orek's user config folder is `$OREK_HOME/config/` when that is set, else
`~/Library/Application Support/orek-ai/` (macOS), `$XDG_CONFIG_HOME/orek-ai/` or
`~/.config/orek-ai/` (Linux), or `%APPDATA%\orek-ai\config\` (Windows).

- **The session-start hook** runs `track prime --hook=<tool>` when a session starts, and again
  after it is cleared or compacted where the tool supports that. In a directory with its own
  `.track.toml` it hands the project briefing to the agent in that tool's hook format; anywhere
  else it adds nothing. It reads the session's folder from the hook input, so it briefs the
  project the agent opened.
- **Agent output.** The Claude Code hook sets `TRACK_OUTPUT=agent` for the session's shell
  commands, Cursor's hook returns it, and Codex gets it in `config.toml`
  (`[shell_environment_policy] set`). Antigravity, Gemini CLI and Orek.ai have no setting for
  it; the instructions tell the agent to add `-o agent`.
- **Codex** runs a hook only after you trust it: open `/hooks` in Codex once.
- **Cursor** also runs Claude Code's hooks. When both are installed, the Claude Code one stays
  quiet inside Cursor so the briefing arrives once.
- **The skill** is a core of under 6 KB (commands, output modes, error handling, the traps) and
  one reference per topic that an agent reads only when it needs it. Tools that share a folder
  share one copy. `track guide <topic>` prints the same references from the binary.
- **The instructions block** tells the agent to run `track prime` before issue work. It is
  static, so it never goes stale; the user-level one applies only in projects with a
  `.track.toml`. A `CLAUDE.md` or `GEMINI.md` that already exists gets an import of `AGENTS.md`
  rather than a second copy.
- Files are edited in place: key order, permissions and everything already in them are kept,
  and a symlinked settings file stays a link to the file that is edited. A file that is not plain
  JSON (Gemini CLI allows comments) is left untouched and reported under its tool; everything
  else is still installed, and the command exits 6.
- `uninstall` removes only what `install` added (and any file or folder it created, once empty).
  With `--tools`, a skill folder or instructions block another tool still reads (`AGENTS.md`,
  `GEMINI.md`, `~/.agents/skills`) is kept, and the output says which tools read it.
- `--skills-only` and `--hooks-only` install one half. `track init --skills` still works and
  installs the skills for you.

Credentials stay in `.track.toml` or environment variables; nothing here touches them.

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

## Agent output (`-o agent`)

`-o agent` is text written for an AI agent to read: no colour, no decoration, and nothing cut
without saying so. Set `TRACK_OUTPUT=agent` to make it the default (an explicit `-o` still
wins; an empty or unknown value is ignored).

```
$ track -o agent i s "project: DEMO #Unresolved" --limit 2
# id	state	priority	assignee	summary
DEMO-1	Open	Normal	jdoe	Implement user authentication
DEMO-2	In Progress	High	-	Fix login timeout
# 2 shown of 57; more may exist: --skip 2, or --all for everything
```

| Command | Agent output |
|---------|--------------|
| `issue search` | One tab-separated line per issue, then a last line that says `complete` or how to get the rest |
| `issue get` | Summary, every set field on one line, then the full description |
| `issue get --full` | The same, plus every link (with its link id) and the last 20 comments in full; more are named with the command that lists them |
| `issue create` / `update` | One line: what was done and the field values the issue ended up with |
| `issue comments`, `project fields`, batch results | One line per item; `project fields` includes the values |
| `context` | The `track prime` briefing |
| everything else | The usual text, uncoloured |

It is typically a third to a fifth of the size of the JSON for the same data.

## Selecting fields (`--select`)

`--select` keeps only the listed fields of whatever JSON a command prints: comma-separated
dotted paths, on any command, with `-o json` or `-o jsonl`. Paths walk into nested arrays, so
they work on the `--full` and `inspect` envelopes too.

```bash
track -o json i s "project: PROJ #Unresolved" --select id_readable,summary,fields.Priority
track -o json PROJ-1 --full --select issue.id_readable,issue.fields.Stage,comments.text
track -o jsonl p ls --select short_name
```

- `fields.<Name>` is an issue's custom field by name, as a plain value
  (`{"fields": {"Priority": "Major"}}`). It exists only under `--select`; the default JSON is
  unchanged. Quote a name that holds `.` or `,`: `fields."Est. time"`.
- A path missing from one record is omitted from that record.
- A selection that matches nothing at all is an error, not `{}`.
- Text output (including `-o agent`) rejects the flag.

When a search fills its limit, a one-line hint with the next `--skip` value (and a total when
one is known) goes to stderr, in JSON and JSONL mode too: a full page of JSON otherwise looks
exactly like a complete result. `-o agent` puts the same verdict on the last line of stdout.

`issue start` / `issue complete` JSON reports what the tracker did, not only what was asked:
`state` is the requested state, `actual_state` the one the issue is in afterwards, `verified`
whether they agree, and `warnings` why not. `id_readable` is the issue id.

## Errors

A failure prints up to three lines on stderr and exits with a code that names its kind:

```
Error: Unknown field 'Prio'. Did you mean 'Priority'? Available fields: State, Priority, Type
Code: unknown_field
Hint: `track prime` lists this project's fields and their values; --no-validate sends the fields as given.
```

`Code:` is a short, stable name to branch on; `Hint:` is the next step, when one is known.
With `-o json` or `-o jsonl` the error is one JSON object on one line of stderr instead:
`{"error": {"code", "message", "hint", "retryable"}}`. `--error-format text|json` (or
`TRACK_ERROR_FORMAT`) chooses explicitly. `retryable` is true when the same command may work
if run again later.

The exit code says what kind of failure it was:

| Exit | Meaning | Codes |
|------|---------|-------|
| 0 | Success (a write the tracker only partly applied still exits 0, with a warning on stderr) | |
| 1 | Any other failure | `api`, `parse`, `io`, `pagination_stalled`, `error` |
| 2 | The command line did not parse | (clap's own message) |
| 3 | Not found | `issue_not_found`, `project_not_found`, `not_found`, `template_not_found` |
| 4 | Not allowed or not set up | `auth`, `permission`, `not_configured`, `token_refused` |
| 5 | Refused as given: fix the command | `unknown_field`, `invalid_value`, `read_only_field`, `invalid_state`, `invalid_input`, `select_no_match`, `unsupported`, `conflict` |
| 6 | Some items of a batch failed | `partial_failure` |
| 7 | Retryable: run it again later | `network`, `rate_limited`, `server` |


| Code | Meaning |
|------|---------|
| `auth` | The token was rejected |
| `permission` | The token is valid but may not do this |
| `issue_not_found`, `project_not_found`, `not_found` | No such issue, project or other resource (or the token cannot see it). `project_not_found` names the closest cached project key |
| `unknown_field`, `invalid_value`, `read_only_field` | Field validation failed; the message lists what is valid |
| `invalid_state` | `issue start` / `complete` asked for a state the project does not have |
| `template_not_found` | No such `-T` template, or it does not apply to this project |
| `select_no_match` | `--select` paths are not in this command's JSON |
| `not_configured`, `token_refused` | No usable config, or a repository config tried to redirect the token |
| `invalid_input`, `unsupported`, `conflict`, `api` | The tracker refused the request |
| `rate_limited`, `server`, `network` | Retryable: the tracker is limiting, failing or unreachable |
| `parse`, `pagination_stalled`, `io` | An unexpected response or a local file problem |
| `error` | Anything else |

Per-issue failures in batch results (`issue update A,B`, `issue inspect`) carry the same
`code` beside their `error` text.

## Workflow hints

State transitions differ per backend. `track context` and `track prime` report each state field's states, in order,
as workflow hints so an agent can move an issue through its lifecycle correctly
without hardcoding state names.
