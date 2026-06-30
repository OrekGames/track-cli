---
title: Query Syntax
description: Issue search query syntax for each track backend.
---

Each backend uses its native query language for `track issue search`.

## YouTrack

```bash
track i s "project: PROJ #Unresolved"
track i s "project: PROJ State: {In Progress}"
track i s "project: PROJ Assignee: me Priority: Major"
```

## Jira (JQL)

```bash
track -b jira i s "project = PROJ AND resolution IS EMPTY"
track -b jira i s "project = PROJ AND status = 'In Progress'"
track -b jira i s "assignee = currentUser() AND priority = Major"
```

## GitHub

```bash
track -b github i s "is:open label:bug"
track -b github i s "is:closed assignee:username"
track -b github i s "is:issue is:open"
```

GitHub uses GitHub's search query syntax (not traditional issue queries).

## GitLab

```bash
track -b gitlab i s "bug fix" --state opened
track -b gitlab i s "performance" --labels "priority::high"
```

GitLab uses project-scoped search with filter parameters.

## Linear

```bash
track -b linear i s "project: ORE #Unresolved"
track -b linear i s "team: ORE state: {In Progress}"
track -b linear i s "project: ORE label: Bug assignee: me"
```

Linear exposes teams as `track` projects. The `Project` field on issues maps to
Linear's native project association (`--field "Project=Track CLI"`).

## Transparent pagination

`issue search`, `issue comments`, `article list`, `article search`, and
`article comments` support an `--all` flag that fetches every page
automatically:

```bash
track i s "project: PROJ #Unresolved" --all      # All unresolved issues
track -o json i s "project: PROJ" --all           # As JSON for scripting
track article list -p PROJ --all                  # All articles in a project
```

The default safety cap is **1000 results**; override it with the
`TRACK_MAX_RESULTS` environment variable. `--all` conflicts with `--limit` and
`--skip`.
