# GitHub

Read when the project uses GitHub Issues (`-b github` / `-b gh`).

| Aspect | Value |
|--------|-------|
| **Flag** | `-b github` / `-b gh` |
| **Auth** | Bearer token (PAT) |
| **Query** | `is:open label:bug` |
| **Issue IDs** | numeric `42` (or `owner/repo#42`) |
| **Knowledge Base** | Yes via repo wiki (no article comments) |
| **Project Creation** | No |
| **Issue Delete** | No (close instead) |
| **Issue Links** | No (use `#number` references) |
| **Subtasks** | Yes (sub-issues API) |

## Setup

**GitHub**:
```toml
backend = "github"

[github]
token = "ghp_xxxxxxxxxxxx"
owner = "your-org"
repo = "your-repo"
# api_url = "https://api.github.com"  # default, omit unless GHE
```

```bash
track init --url https://api.github.com --token ghp_TOKEN --backend github --project your-org/your-repo
```

| Backend | Variables |
|---------|-----------|
| GitHub | `GITHUB_TOKEN`, `GITHUB_OWNER`, `GITHUB_REPO`, `GITHUB_API_URL` |

## Query syntax

```bash
track -b gh i s "is:open" --limit 20
track -b gh i s "is:open label:bug"
track -b gh i s "is:open assignee:username"
track -b gh i s "is:open memory leak"
```

## Notes

- Use numeric issue IDs (`42`), never project-prefixed keys — `track -b gh i g PROJ-42` is a hard error. GitHub also accepts `owner/repo#42`; GitLab also accepts `#42`.
- GitHub: no delete (close with `track -b gh i done 42` or `--state closed`), links only for subtask/parent (reference via `#42` in comments for other relationships)
- Search/get/comment scope is implicit from config (`owner/repo` or `project_id`), but **`issue create` still requires `-p` or `default_project`** (which `track init` does not set for these backends)
- **GitHub limitations**: No issue delete (close instead), no general issue links (only subtask/parent; use `#N` references); articles work via repo wiki (no article comments)
- Only the state and `Assignee` can be set with field flags (`--state`, `--assignee`, `--field Status=...`); anything else is refused before the write. Use `--tag <label>` for labels.
- State values: `open`, `closed`, and the aliases `done`/`resolved`/`completed` (closed) and `in progress`/`develop`/`reopened` (open).
