# JSON output shapes and `--select`

Read before parsing `-o json` output or narrowing it with `--select`.

## JSON Output Shapes

`-o json` is the mode for programmatic use; `-o jsonl` prints the same shapes compacted (one array element per line). Key shapes:

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
- **`i s -o json`** returns a bare array; when the page is full, a hint with the next `--skip` goes to stderr.
- **`i start` / `i done -o json`**: `issue start` / `issue complete` JSON reports what the tracker did, not only what was asked: `state` is the requested state, `actual_state` the one the issue is in afterwards, `verified` whether they agree, and `warnings` why not. `id_readable` is the issue id (`idReadable` is kept for older scripts).
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
- For a batch view across issues, use `track i ix <ids> --include history` instead of looping `i history` per issue.
- **`from` coverage varies by backend.** Jira/YouTrack/Linear carry the prior value for every field. GitHub/GitLab are event-based, so `from` is populated only for `status` (derived from the event sequence); other fields report `from: null` with the new value in `to`. Linear label-change history is not yet included.

**Inspect** (`i ix`, multi-issue report):
```json
{
  "total": 3, "succeeded": 2, "failed": 1,
  "issues": [
    { "id": "10234", "id_readable": "PROJ-123", "summary": "...",   // full issue fields, flattened
      "comments": [...], "links": [...], "subtasks": [...], "history": [...],  // only requested includes
      "warnings": [{"include": "history", "message": "..."}] }      // unsupported include capability
  ],
  "errors": [{"id": "PROJ-404", "error": "Issue not found: ..."}]
}
```

- Each `issues[]` entry is the standard Issue shape plus one array per requested `--include`; unsupported include capabilities are explained in that issue's `warnings`, while other include fetch failures become per-issue errors.
- `subtasks` is a links-derived view: the subset of `links` whose type is a subtask/parent relationship.
- Exit code is 0 even with failures unless `--strict` (reports everything first, then exits 1).
- **`--jsonl`** (overrides `-o`): one compact object per line — success lines are the `issues[]` entry plus `"success": true`; failures are `{"success": false, "id", "error"}`. Plain `-o jsonl` instead prints the report above on a single line.

## `--select` projection (JSON/JSONL only)

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
