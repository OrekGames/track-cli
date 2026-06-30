---
title: Backends
description: Backend-specific behavior and quirks for YouTrack, Jira, GitHub, GitLab, and Linear.
---

`track` presents one command set across five trackers, but each backend has its
own authentication, identifiers, and capabilities. This page covers what differs.

## YouTrack

- Full feature support including custom fields, field admin, and knowledge base.
- Bearer token authentication.
- Rich query language for issue search.

## Jira

- **Knowledge Base:** uses the Confluence API (at the same domain with a `/wiki`
  path).
- **Authentication:** Basic Auth with email and API token.
- **Rich Text:** uses Atlassian Document Format (ADF) for descriptions. ADF
  rich-text custom fields are surfaced as rendered plain text, the same way
  descriptions and comments are.
- **Project Creation:** requires admin permissions (use the web interface).
- **Subtasks:** create with `--parent`, or link existing issues with
  `issue link -t subtask`.
- **Labels:** map to tags.
- **System & custom fields:** `issue get`/`issue search` surface *all* populated
  fields Jira returns — standard system fields and every custom field — as
  `custom_fields` entries. Anything that can't be mapped to a typed variant is
  preserved verbatim, so no data is lost.
- **Components:** surfaced as a `Components` multi-value custom field. To filter
  by area, use server-side JQL such as `component = "Rendering"`.

## GitHub

- **Scope:** repository-scoped (requires owner and repo configuration).
- **Issue IDs:** numeric issue numbers (e.g. `42`), not project-prefixed keys.
- **Labels:** map to tags with color support.
- **No issue deletion:** GitHub does not support deleting issues (close them
  instead).
- **Subtasks:** supported via the sub-issues API (`--parent`,
  `issue link -t subtask/parent`).
- **No general issue links:** reference related issues via `#number` in comments.
- **Pull requests:** automatically filtered out from issue lists.
- **Rate limiting:** use authenticated requests to avoid public API limits.

## GitLab

- **Scope:** project-scoped via `project_id` configuration.
- **Issue IDs:** uses IID (project-scoped, e.g. `#42`), not global IDs.
- **Labels:** map to tags with color support (includes `#` prefix).
- **Comments:** called "notes" in the GitLab API; system notes are filtered out.
- **API Version:** GitLab REST API v4 (with GraphQL for parent-child).
- **Subtasks:** supported via the GraphQL API (`--parent`,
  `issue link -t subtask/parent`).

## Linear

- **Scope:** team-scoped for CLI projects (`-p ORE` maps to a Linear team).
- **API:** Linear GraphQL with personal API keys
  (`Authorization: <API_KEY>`).
- **Projects:** Linear projects are issue associations, set with
  `--field "Project=Track CLI"` or `linear.default_linear_project`.
- **Labels:** map to tags; unknown labels on create/update are rejected.
- **Subtasks and Links:** parent-child uses `parentId`; relation links support
  `related`, `blocks`, `duplicate`, and `similar`.
- **Knowledge Base:** article commands are not supported.
