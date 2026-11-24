# Declarative apply plans

Read when a task needs several ordered, dependent writes: `track apply plan.json`.

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
- Fields are validated in the default mode (see Field Validation). `--validate` or `defaults.validate: true` makes it strict; `--no-validate` or `defaults.validate: false` turns it off. The command line wins over the plan.
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

**Field values** in `fields`: a string sets one value (`"Priority": "Major"`), an array sets multiple (`"Platform": ["macOS", "Linux"]`), and an **empty array clears the field** (`"Platform": []`).

JSON output includes `success`, `dry_run`, `resumed`, summary counts, `refs`, and per-operation results with `index`, `op`, `status`, `issue`, `ref`, `error`, and `warnings`. Stop on the first failed operation; resume skips completed operations when the checksum still matches.
