# YouTrack

Read when the project uses YouTrack (the default backend).

| Aspect | Value |
|--------|-------|
| **Flag** | `-b youtrack` / `-b yt` (default) |
| **Auth** | Bearer token |
| **Query** | `project: PROJ #Unresolved` |
| **Issue IDs** | `PROJ-123` |
| **Knowledge Base** | Yes (`article` commands) |
| **Project Creation** | Yes |
| **Issue Delete** | Yes |
| **Issue Links** | Yes |
| **Subtasks** | Yes |

## Setup

**YouTrack**:
```toml
backend = "youtrack"
url = "https://youtrack.example.com"
token = "perm:xxx"
default_project = "PROJ"
```

```bash
track init --url https://youtrack.example.com --token YOUR_TOKEN --project PROJ
```

| Backend | Variables |
|---------|-----------|
| YouTrack | `YOUTRACK_URL`, `YOUTRACK_TOKEN` |

## Query syntax

```bash
track i s "project: PROJ #Unresolved" --limit 20
track i s "project: PROJ State: {In Progress}"
track i s "project: PROJ Assignee: me"
track i s "project: PROJ #Unresolved Priority: Major"
```

## Notes

- Field names and values are applied exactly as given, so validation is strict here: an unknown field or a value outside a field's bundle is refused before the write.
- The state field is `State` on some projects and `Stage` on others; `track prime` names it.
- Custom field and bundle administration (`track field`, `track bundle`) exists only on this backend: see `admin`.
