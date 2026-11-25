---
name: track
description: Use the `track` CLI to read and change issues in YouTrack, Jira, GitHub, GitLab or Linear. Covers the command grammar, output modes, error codes and the traps; backend details load on demand.
user-invokable: true
disable-model-invocation: false
---

# track

`track` is one CLI for five issue trackers. The project's `.track.toml` picks the backend; `-b <backend>` overrides it per command.

## First: get the project's facts

```bash
track prime
```

It prints, from the local cache, the real state names, the fields required on create, every enum value, assignable users, tags, `-T` templates, and example commands in this backend's query syntax. Use those names; do not guess them. If it says there is no cache, run `track cache refresh` once. A session hook may already have printed it.

## Commands

| Task | Command |
|------|---------|
| Read an issue | `track PROJ-123 --full` (numeric ids: `track i g 42 --full`) |
| Search | `track i s "<query>"` or `track i s -T <template>` (`--limit N`, `--skip N`, `--all`) |
| Create | `track i new -s "Title" [-p PROJ] [--field Name=Value]... [--tag T]... [--parent ID]` |
| Update | `track i u ID[,ID...] [--summary ...] [--field Name=Value]... [--state S] [--assignee U]` |
| Start / finish | `track i start ID [--state S]` · `track i done ID [--state S]` |
| Comment | `track i cmt ID -m "text"` · list: `track i comments ID --all` |
| Link | `track i link SRC DST [-t relates\|depends\|duplicates\|subtask\|parent]` |
| Many issues at once | `track i ix ID,ID --include comments,links,history` |
| Projects, fields, tags | `track p ls` · `track -o json p f PROJ` · `track t ls` |
| Long text | `--body-file FILE` (`-` reads stdin) on create, update and comment |

Query syntax differs per backend and is passed through as written; `track prime` shows a working example. `track <command> -h` lists a command's own options.

## Output

- `-o agent` (or `TRACK_OUTPUT=agent`): compact text. One line per search hit, whole descriptions and comments, and a last line that says whether a list is complete. Prefer it for reading.
- `-o json` / `-o jsonl`: for parsing. Add `--select id_readable,summary,fields.Priority` to keep only what you need; `fields.<Name>` is a custom field by name.
- stdout is only the result. Warnings, pagination hints and errors go to stderr, in JSON mode too.
- Exit code 0 is success; 3 not found, 4 not allowed, 5 fix the command, 6 part of a batch failed, 7 retry later, 1 anything else. A failure prints `Error:`, a stable `Code:` and often a `Hint:` with the next step (one JSON object in JSON mode). Follow the hint.

## Rules that prevent mistakes

1. Use `id_readable` (`PROJ-123`) in commands, never the internal `id`.
2. Field names and values are checked before a write. `Unknown field` and `Invalid value` errors list what is valid; fix the command, do not retry it unchanged. `--dry-run` checks without writing.
3. `i start` / `i done` fail and list the project's states when the default state does not exist there: pass `--state "<name>"`.
4. A search returns 20 results unless told otherwise. A full page is not the whole result: read the last line (agent mode) or stderr hint, then use `--skip` or `--all`.
5. `i comments` returns 10 by default; use `--all`.
6. A write that the tracker only partly applied still exits 0, with `⚠ Warning:` on stderr. Read stderr after every write.
7. GitHub and GitLab use numeric ids (`42`), set labels with `--tag`, and can set only the state and assignee through field flags.
8. `resolved` in JSON is a timestamp and can be null on finished issues; use the state field's `is_resolved`.
9. Do not run `track config trust`, and do not pass `--token`: credentials are the user's to manage.
10. If output looks canned, check that `TRACK_MOCK_DIR` is unset.

## References

Read the one that matches the task. Each is a file in `references/` beside this one, and `track guide <topic>` prints the same text.

| Topic | Read when |
|-------|-----------|
| `youtrack`, `jira`, `github`, `gitlab`, `linear` | You need that backend's query syntax, id format, setup or limits |
| `queries` | Translating a search between backends; `-T` templates and workflow packs |
| `issues` | Attachments, raw JSON payloads, batches, pagination, validation modes |
| `json` | Parsing `-o json`: the shape of issues, history, inspect reports; `--select` |
| `output` | The exact stdout/stderr contract, `-o agent`, every error code |
| `apply` | Several ordered, dependent writes in one plan file |
| `articles` | Knowledge base and wiki pages |
| `admin` | Projects, tags, YouTrack fields and bundles, link types, all aliases |
| `config` | Setup, environment variables, `track doctor`, config trust |
| `cache` | What is cached, how lookups read through it, `track context` |
