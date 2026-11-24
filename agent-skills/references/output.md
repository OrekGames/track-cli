# Output modes, streams and errors

Read for the exact output contract: what goes to stdout and stderr, `-o agent`, and every error code.

## Exit Codes & Output Streams

- **Exit code**: `0` = success, `1` = any failure. A partially-applied update (some fields ignored by the backend) still exits `0` — with a warning on stderr.
- **stdout** carries only the result: pretty-printed JSON in `-o json` mode, or the same document as compact JSON Lines in `-o jsonl` mode. JSONL is root-aware: an array (list commands such as `i s`, `p ls`, `i comments`) prints one element per line and an empty array prints nothing; anything else (an object such as `i g`, a batch summary, a report) prints on a single line. Nested arrays stay inside their line. Every command that prints JSON accepts `-o jsonl`; `completions` rejects it.
- **stderr** carries ALL diagnostics as plain text — errors, `⚠ Warning:` lines, pagination hints, progress — **even in JSON mode**. Never parse stdout for error objects; check the exit code and read stderr.

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

## Errors

A failure prints up to three lines on stderr and exits non-zero:

```
Error: Unknown field 'Prio'. Did you mean 'Priority'? Available fields: State, Priority, Type
Code: unknown_field
Hint: `track prime` lists this project's fields and their values; --no-validate sends the fields as given.
```

`Code:` is a short, stable name to branch on; `Hint:` is the next step, when one is known.
`--error-format json` (or `TRACK_ERROR_FORMAT=json`) prints one JSON object on one line
instead: `{"error": {"code", "message", "hint", "retryable"}}`. `retryable` is true when the
same command may work if run again later.

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
