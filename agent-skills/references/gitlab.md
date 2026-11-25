# GitLab

Read when the project uses GitLab Issues (`-b gitlab` / `-b gl`).

| Aspect | Value |
|--------|-------|
| **Flag** | `-b gitlab` / `-b gl` |
| **Auth** | Private token |
| **Query** | `state=opened&labels=bug` |
| **Issue IDs** | numeric `42` (or `#42`) |
| **Knowledge Base** | Yes via wiki (no article comments) |
| **Project Creation** | No |
| **Issue Delete** | Yes |
| **Issue Links** | Yes |
| **Subtasks** | Yes (GraphQL API) |

## Setup

**GitLab**:
```toml
backend = "gitlab"

[gitlab]
token = "glpat-xxxxxxxxxxxx"
url = "https://gitlab.com/api/v4"
project_id = "12345"
# namespace = "your-group"
```

```bash
track init --url https://gitlab.com/api/v4 --token glpat-TOKEN --backend gitlab --project 12345
```

| Backend | Variables |
|---------|-----------|
| GitLab | `GITLAB_TOKEN`, `GITLAB_URL`, `GITLAB_PROJECT_ID`, `GITLAB_NAMESPACE` |

## Query syntax (filters)

Honored `GET /projects/:id/issues` keys: `state`, `labels`, `search`, `assignee_username`, `order_by`, `sort`. Other keys warn on stderr and are ignored.

`assignee_username=@me` or `me` becomes `scope=assigned_to_me`. Any other value is sent as `assignee_username[]` (GitLab's string-array form; CE accepts one username).

```bash
track -b gl i s "state=opened" --limit 20
track -b gl i s "labels=bug"
track -b gl i s "state=opened&labels=bug,critical"
track -b gl i s "state=opened&assignee_username=@me"
track -b gl i s "state=opened&order_by=updated_at&sort=desc"
track -b gl i s "search=memory leak"
```

## GitLab write-side assignees

`issue create`/`update` `--assignee` (and the same fields in `apply` plans) accept:

- `@username` or `username` — resolved case-insensitively (project members, then `GET /users?username=`)
- numeric user id — verified via `GET /users/:id`; an unknown id fails the operation (GitLab would otherwise silently drop it)
- `""`, `"0"`, or `"@0"` — clear assignees (sent as an empty `assignee_ids` array)

```bash
track -b gl i new -p 123 -s "Title" --assignee @username
track -b gl i u 42 --assignee 7
track -b gl i u 42 --assignee ""            # clear
```

## Notes

- Use numeric issue IDs (`42`), never project-prefixed keys — `track -b gh i g PROJ-42` is a hard error. GitHub also accepts `owner/repo#42`; GitLab also accepts `#42`.
- GitHub: no delete (close with `track -b gh i done 42` or `--state closed`), links only for subtask/parent (reference via `#42` in comments for other relationships)
- Search/get/comment scope is implicit from config (`owner/repo` or `project_id`), but **`issue create` still requires `-p` or `default_project`** (which `track init` does not set for these backends)
- **GitLab limitations**: No project creation; query filters limited to `state`/`labels`/`search`; articles work via wiki (no article comments)
- Only the state and `Assignee` can be set with field flags; anything else is refused before the write. Use `--tag <label>` for labels.
