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

## Context aggregation

`track context` returns everything an agent needs to reason about a tracker in a
single call: projects, custom fields (with enum values), assignable users, query
templates, workflow hints (valid state transitions), issue counts, and recently
accessed issues.

```bash
track context                  # Aggregated context
track context --project PROJ   # Scope to one project
track -o json context          # JSON for parsing
track context --include-issues # Include unresolved issues
```

A local cache backs this for fast, low-API-cost lookups:

```bash
track cache refresh --if-stale 1h
track cache status
```

## Batch operations

Operate on many issues in one command — ideal for agents applying a plan:

```bash
track i u PROJ-1,PROJ-2,PROJ-3 --field "Priority=Major"  # Batch update
track i start PROJ-1,PROJ-2,PROJ-3                        # Batch start
track i done PROJ-1,PROJ-2 --state Done                   # Batch complete
track i del PROJ-1,PROJ-2,PROJ-3                          # Batch delete
```

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
track init --skills           # Install skills only; no tracker config change
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

## Workflow hints

State transitions differ per backend. `track context` reports valid transitions
as workflow hints so an agent can move an issue through its lifecycle correctly
without hardcoding state names.
