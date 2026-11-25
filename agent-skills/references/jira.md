# Jira

Read when the project uses Jira (`-b jira` / `-b j`), or Confluence for articles.

| Aspect | Value |
|--------|-------|
| **Flag** | `-b jira` / `-b j` |
| **Auth** | Basic Auth (email + API token) |
| **Query** | JQL: `project = PROJ AND resolution IS EMPTY` |
| **Issue IDs** | `PROJ-123` |
| **Knowledge Base** | Yes via Confluence |
| **Project Creation** | No |
| **Issue Delete** | Yes |
| **Issue Links** | Yes |
| **Subtasks** | Yes |

## Setup

**Jira**:
```toml
backend = "jira"
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"
```

```bash
track init --url https://company.atlassian.net --token API_TOKEN --backend jira --email you@example.com
```

| Backend | Variables |
|---------|-----------|
| Jira | `JIRA_URL`, `JIRA_EMAIL`, `JIRA_TOKEN` |

## Query syntax (JQL)

```bash
track -b j i s "project = PROJ AND resolution IS EMPTY" --limit 20
track -b j i s "project = PROJ AND status = 'In Progress'"
track -b j i s "assignee = currentUser()"
track -b j i s "project = PROJ AND statusCategory != Done ORDER BY created DESC"
```

**Passthrough rule**: a query containing `=`, ` AND `, or ` OR ` is sent to Jira **verbatim**, so the full JQL surface evaluates server-side (`ORDER BY`, `statusCategory`, `created >= -90d`, `status CHANGED`, sprint functions, ...). Queries without those tokens are auto-converted from YouTrack-style syntax (`project: PROJ #Unresolved` works on Jira too; leftover words become `text ~ "..."`).

**Traps**:
- A sort-only query like `"ORDER BY created DESC"` contains no `=`/`AND`/`OR` and gets mangled into a text search — anchor it: `"project = PROJ ORDER BY created DESC"`.
- `#Unresolved` maps to `resolution IS EMPTY`, which still matches Done issues whose workflow never set a resolution. Use `statusCategory != Done` for true open-ness.

## Notes

- **Jira limitations**: No project creation, no custom field admin, no total counts from search
- **Confluence IDs**: Numeric page IDs and space IDs, not project keys
- **Resolution vs closed**: the JSON `resolved` field is a timestamp that can be null on Done issues — use the State field's `is_resolved` for closedness
- `Priority`, `Type` and `Components` values are read from the instance and checked before a write; the status is left to the API because it also accepts transition names.
