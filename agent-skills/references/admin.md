# Projects, tags, fields, links and aliases

Read for project and tag management, YouTrack field and bundle administration, link types, and the full alias table.

## Project Operations

| Operation | Command |
|-----------|---------|
| List | `track p ls` (add `-b gh`/`-b gl` for other backends) |
| Get | `track p g PROJ` |
| Fields | `track -o json p f PROJ` (text mode omits valid values — use JSON, `cache show`, or `context`) |
| Create | `track p new -n "Name" -s "KEY" [-l "leader"]` (YouTrack only; leader defaults to authenticated user) |
| Attach existing field | `track p attach-field PROJ -f <FIELD_ID> --bundle <BUNDLE_ID> [--required]` (YouTrack; `--bundle` required for enum/state) |

## Tag Management

```bash
track t ls                                                   # List tags
track t create "regression" --tag-color "#d73a4a" -d "desc"  # Create (hex with or without #)
track t update "regression" --new-name "reg" --tag-color d73a4a
track t rm "regression"
```

**Careful**: the tag color flag is `--tag-color`; `--color` is the global terminal-color mode (`auto|always|never`).

## Custom Field Admin (YouTrack Only)

| Operation | Command |
|-----------|---------|
| List fields | `track field ls` |
| Create field | `track field create "Name" -t enum` |
| Create + attach | `track field new "Name" -t enum -p PROJ --values "Val1,Val2,Val3"` |
| List bundles | `track bundle ls -t enum` |
| Create bundle | `track bundle create "Name" -t enum --values "Val1,Val2"` |
| Add value | `track bundle add-value <BUNDLE_ID> -t enum --value "NewValue"` |

**Careful**: values flags are `--values` (field new, bundle create) and `--value` (bundle add-value) — `-v` is the global `--verbose` flag. `field new` requires both `--project` and `--values`.

**Field types**: `enum`, `multi-enum`, `state`, `text`, `date`, `integer`, `float`, `period`

**State fields with resolved markers**:
```bash
track field new "Status" -t state -p PROJ --values "Open,In Progress,Done" --resolved "Done"
track bundle create "Bug Status" -t state --values "Open,Fixed,Closed" --resolved "Fixed,Closed"
```

## Link Type Mappings

Built-in link types: `relates`, `depends`, `required`, `duplicates`, `duplicated-by`, `subtask`, `parent`. Unrecognized types pass through to the backend as-is (for admin-defined types like `clones`).

Override default canonical-to-native mappings in config (file-only tables — not settable via `config set`):

```toml
[jira.link_mappings]
depends = "Requires"       # Default: "Blocks"

[youtrack.link_mappings]
depends = "Is required for" # Default: "Depend"

[gitlab.link_mappings]
depends = "is_blocked_by"  # Default: "blocks"

[linear.link_mappings]
depends = "blocks"         # Default: "blocks"
```

Default mappings:

| Canonical | YouTrack | Jira | GitLab | Linear |
|-----------|----------|------|--------|--------|
| `relates` | `Relates` | `Relates` | `relates_to` | `related` |
| `depends` | `Depend` | `Blocks` | `blocks` | `blocks` |
| `required` | `Depend` | `Blocks` | `is_blocked_by` | `blocks` |
| `duplicates` | `Duplicate` | `Duplicate` | `relates_to` | `duplicate` |
| `duplicated-by` | `Duplicate` | `Duplicate` | `relates_to` | `duplicate` |

**Finding a LINK_ID for unlink**: `track -o json PROJ-1 --full` → `links[].id`. (There is no `issue links` subcommand, despite what `issue unlink --help` implies.)

## Command Aliases

| Full | Aliases |
|------|---------|
| `track issue` | `track i` |
| `track issue get` | `track i g` |
| `track issue create` | `track i new`, `track i c` |
| `track issue update` | `track i u` |
| `track issue search` | `track i s`, `track i find` |
| `track issue inspect` | `track i ix` |
| `track issue delete` | `track i rm`, `track i del` |
| `track issue comment` | `track i cmt` |
| `track issue comments` | `track i comments` |
| `track issue history` | `track i history`, `track i hist` |
| `track issue complete` | `track i done`, `track i resolve` |
| `track issue start` | `track i start` |
| `track issue link` | `track i link` |
| `track issue unlink` | `track i ul` |
| `track project` | `track p` |
| `track project list` | `track p ls` |
| `track project get` | `track p g` |
| `track project fields` | `track p f` |
| `track tags list` | `track t ls` |
| `track config` | `track cfg` |
| `track article` | `track a`, `track wiki` |
| `track context` | `track ctx` |
| `track field list` | `track field ls` |
| `track bundle list` | `track bundle ls` |

## Other Commands

```bash
track open PROJ-123        # Open in browser (no ID = dashboard); -o json returns {"success":bool,"url":...}
track completions zsh      # Shell completions (bash|zsh|fish|powershell|elvish); no config/API needed
```
