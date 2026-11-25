# Linear

Read when the project uses Linear (`-b linear` / `-b lin`).

| Aspect | Value |
|--------|-------|
| **Flag** | `-b linear` / `-b lin` |
| **Auth** | Personal API key |
| **Query** | `project: ORE #Unresolved` |
| **Issue IDs** | `ORE-123` |
| **Knowledge Base** | No |
| **Project Creation** | No (teams are projects) |
| **Issue Delete** | Yes |
| **Issue Links** | Yes |
| **Subtasks** | Yes (`parentId`) |

## Setup

**Linear**:
```toml
backend = "linear"
default_project = "ORE" # Linear team key/name/id

[linear]
token = "lin_api_xxxxxxxxxxxx"
url = "https://linear.app/your-workspace" # used by track open
# api_url = "https://api.linear.app/graphql"
# default_linear_project = "Track CLI"
```

```bash
track init --url https://linear.app/your-workspace --token lin_api_TOKEN --backend linear --project ORE
```

| Backend | Variables |
|---------|-----------|
| Linear | `LINEAR_TOKEN`, `LINEAR_API_URL`, `LINEAR_URL`, `LINEAR_DEFAULT_TEAM`, `LINEAR_DEFAULT_PROJECT` |

## Query syntax

```bash
track -b lin i s "project: ORE #Unresolved"
track -b lin i s "team: ORE state: {In Progress}"
track -b lin i s "project: ORE label: Bug assignee: me"
track -b lin i s "project: ORE priority: High"
```

## Notes

- Use readable identifiers like `ORE-123`; teams are exposed as `track` projects.
- Native Linear Project is set with `--field "Project=Track CLI"` or `linear.default_linear_project`.
- Unknown labels on create/update are rejected; inspect valid values with `track -o json p f ORE`.
- **Linear limitations**: No team creation, no knowledge base; native Linear Project is a field/association, not the CLI project
- Settable fields: Status (also `State`/`Stage`), Assignee, Priority, Labels, Project. State and priority values are checked before a write.
