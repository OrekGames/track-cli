# Agent integrations: one design across coding tools

`track agent install` puts `track` in front of seven coding agents. Each gets the same
three things, delivered through whatever that tool documents, and nothing that the tool's
own documentation does not describe:

1. **A skill**: `SKILL.md` (under 6 KB) plus `references/`, loaded when the agent needs it.
2. **A session-start hook** running `track prime --hook=<tool>`, which hands the agent the
   project briefing (real state names, required fields, values, users) in that tool's hook
   format, and adds nothing outside a directory with a `.track.toml`.
3. **Instructions**: a static marker block telling the agent to run `track prime` before
   issue work, for sessions where no hook ran.

Plus the output default: `TRACK_OUTPUT=agent` where the tool has a way to set it.

## What each tool documents

Checked against each tool's documentation in November 2026.

| | Orek.ai | Claude Code | Codex | Cursor | Antigravity | Gemini CLI |
|---|---|---|---|---|---|---|
| Skills (user) | `<orek config>/skills`, `~/.agents/skills` | `~/.claude/skills` | `~/.agents/skills` | `~/.agents/skills`, `~/.cursor/skills` | `~/.gemini/config/skills` | `~/.agents/skills`, `~/.gemini/skills` |
| Skills (project) | `.orek/skills`, `.agents/skills` | `.claude/skills` | `.agents/skills` | `.agents/skills`, `.cursor/skills` | `.agents/skills` | `.agents/skills`, `.gemini/skills` |
| Hook file (user) | none yet | `~/.claude/settings.json` | `$CODEX_HOME/hooks.json` | `~/.cursor/hooks.json` | `~/.gemini/config/hooks.json` | `~/.gemini/settings.json` |
| Hook file (project) | none yet | `.claude/settings.json` | `.codex/hooks.json` | `.cursor/hooks.json` | `.agents/hooks.json` | `.gemini/settings.json` |
| Hook shape | — | `hooks.SessionStart[].hooks[]` | same as Claude Code | `{"version":1,"hooks":{"sessionStart":[…]}}` | `{"<group>":{"SessionStart":[…]}}` | same as Claude Code |
| Context output | — | `hookSpecificOutput.additionalContext` (or plain stdout) | same (or plain stdout) | `additional_context` | `injectSteps[].systemMessage` | `hookSpecificOutput.additionalContext`; stdout must be JSON |
| Session folder in input | — | `cwd` | `cwd` | `workspace_roots` | `workspacePaths` | `cwd` |
| Re-runs after | — | resume, clear, compact | resume, clear, compact, fork (by matcher) | — | — | resume, clear |
| Timeout unit | — | seconds | seconds | seconds | seconds | milliseconds |
| Instructions (user) | none | `~/.claude/CLAUDE.md` | `$CODEX_HOME/AGENTS.md` (or `AGENTS.override.md`) | UI only | `~/.gemini/GEMINI.md` | `~/.gemini/GEMINI.md` |
| Instructions (project) | `AGENTS.md` | `CLAUDE.md`; `AGENTS.md` only without one | `AGENTS.md` | `AGENTS.md` | `AGENTS.md`, `GEMINI.md` | `GEMINI.md` (`@file` imports) |
| Env for shell commands | inherits the shell | `CLAUDE_ENV_FILE` from a SessionStart hook | `[shell_environment_policy] set` | hook `env` (documented for later hooks) | none | none |

`<orek config>` is `$OREK_HOME/config/` when set, else `~/Library/Application Support/orek-ai/`
(macOS), `$XDG_CONFIG_HOME/orek-ai/` or `~/.config/orek-ai/` (Linux), `%APPDATA%\orek-ai\config\`
(Windows).

Sources:
[Orek.ai skills](https://orek.ai/docs/skills/),
[Orek.ai configuration](https://orek.ai/docs/reference/configuration/),
[Claude Code hooks](https://code.claude.com/docs/en/hooks),
[Codex hooks](https://learn.chatgpt.com/docs/hooks),
[Codex config](https://learn.chatgpt.com/docs/config-file/config-reference),
[Codex AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md),
[Codex skills](https://learn.chatgpt.com/docs/build-skills),
[Cursor hooks](https://cursor.com/docs/hooks),
[Cursor rules](https://cursor.com/docs/rules),
[Cursor skills](https://cursor.com/docs/skills),
[Antigravity hooks](https://antigravity.google/docs/hooks/),
[Antigravity rules](https://antigravity.google/docs/rules/),
[Antigravity skills](https://antigravity.google/docs/skills/),
[Gemini CLI hooks](https://geminicli.com/docs/hooks/reference/),
[Gemini CLI GEMINI.md](https://geminicli.com/docs/cli/gemini-md/),
[Gemini CLI skills](https://geminicli.com/docs/cli/skills/).

GitHub Copilot gets the skill in `~/.copilot/skills/` as before; its hook and instruction
files were not verified, so nothing else is written for it.

## Decisions

- **One skill copy per folder.** Codex, Cursor and Gemini CLI all read `~/.agents/skills`, so
  they share one copy. Copies 1.x (`track init --skills`) wrote to `~/.cursor/skills/track` and
  `~/.gemini/skills/track` are removed (only if their `SKILL.md` says `name: track`), so those
  tools do not load the skill twice. Orek.ai gets its own folder because it is the first one
  Orek reads.
- **One command, one flag per tool.** `track prime --hook=<tool>` prints exactly one JSON
  document in that tool's format, or `{}` when it has nothing to add. `--hook` alone means
  `claude`. It never fails: a parse error, a missing config or an invalid pack all print `{}`
  and exit 0.
- **The hook briefs the session's folder.** It reads `cwd`, `workspace_roots[0]` or
  `workspacePaths[0]` from the JSON object on stdin (non-terminal stdin only; reading stops at
  the object's closing brace, and waits half a second at most for an agent that sends nothing)
  and moves there before loading config, so a user-level hook briefs the project the agent
  opened.
- **The briefing is this tracker's.** A cache written for another instance (the config's URL or
  repository changed, or a `.tracker-cache/` was committed) is not used: the briefing says to
  run `track cache refresh`, and `cache refresh --if-stale` refreshes it however recent it is.
- **No double briefing in Cursor.** Cursor also runs Claude Code's hooks. The Claude Code hook
  prints `{}` when its input carries `cursor_version` and a Cursor hook of ours is installed.
- **Instructions without duplication.** Project scope writes the block to `AGENTS.md`, which
  Orek.ai, Codex, Cursor and Antigravity read. An existing `CLAUDE.md`, `.claude/CLAUDE.md` or
  `GEMINI.md` gets a short `@AGENTS.md` import block instead of a copy; none is created. The
  user-level block is conditional ("in a project with a `.track.toml`").
- **Agent output.** Claude Code: the hook appends `export TRACK_OUTPUT=agent` to
  `CLAUDE_ENV_FILE` (once). Cursor: the hook returns it in `env`. Codex: `config.toml` gets
  `shell_environment_policy.set.TRACK_OUTPUT = "agent"` unless the user set a value. Nothing
  is written to a settings file's own `env`.
- **Codex trust.** Codex runs non-managed hooks only after the user trusts them in `/hooks`;
  the installer says so.
- **Edits are reversible.** JSON files keep their key order and untouched values; TOML is
  edited with `toml_edit`; Markdown only inside the markers. Uninstall removes only what install
  added, deletes files it leaves empty, and in project scope removes the folders on those paths
  once they are empty.
- **The user's files stay the user's.** A file keeps its permissions and a folder is never
  chmod-ed; a symlinked settings file stays a link, and the file it points at is the one edited
  (emptied rather than deleted on uninstall). Only track's own cache is written private.
- **A file that cannot be round-tripped is not rewritten.** A settings file that is not plain
  JSON (Gemini CLI allows comments) is left byte for byte, reported under its tool, and every
  other file is still installed; the command then exits 6 (`partial_failure`).
- **Shared files outlive a single tool.** `uninstall --tools X` keeps a skill folder or
  instructions block (`~/.agents/skills`, `AGENTS.md`, `GEMINI.md`) while another tool that
  reads it is not being uninstalled, and says which.

## Orek.ai session-start hook (proposed)

Orek.ai has no hooks today. `track prime --hook=orek` already prints the shape below, so
`track` works the day Orek ships it; until then Orek.ai is covered by the skill and the
`AGENTS.md` block, and `track agent status` reports its hook as not available.

The proposal follows the shape Claude Code, Codex and Gemini CLI share, so one hook command
serves all of them:

```toml
# <orek config>/config.toml, or .orek/config.toml in a project
[[hooks.session_start]]
command = "track prime --hook=orek"
timeout = 10          # seconds
# matcher = "startup|resume|clear|compact"   # optional, by session source
```

- **When:** at session start, and again after `/clear` and after context compaction (the
  moments `AGENTS.md` is re-read), with the reason in `source`.
- **Input** on stdin, one JSON object:
  `{"hook_event_name": "SessionStart", "source": "startup", "session_id": "…", "cwd": "/path/to/project"}`.
- **Output** on stdout, one JSON object:
  `{"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": "…"}}`.
  The text is added to the model's context, as `AGENTS.md` is. Empty stdout or `{}` adds
  nothing. A non-zero exit or a timeout is logged and ignored; it never blocks the session.
- **Size:** context from hooks capped (Claude Code uses 10,000 characters).

When Orek.ai ships this, `Roots::hook_target` gains its paths and `track agent install` writes
the entry; nothing else in `track` changes.
