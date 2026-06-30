---
title: Overview
description: track is a fast Rust CLI for YouTrack, Jira, GitHub, GitLab, and Linear with one unified command interface.
---

`track` is a command-line interface for issue tracking systems, built in Rust.
It speaks to **YouTrack**, **Jira**, **GitHub**, **GitLab**, and **Linear**
through a single, unified set of commands — so the way you get, create, search,
and update issues is the same no matter which backend your team uses.

## Why track

- **Multi-Backend** — five trackers, one command set.
- **Issue Management** — get, create, update, delete, and search issues.
- **Batch Operations** — update, delete, or complete many issues at once, plus
  declarative bulk `apply`.
- **Transparent Pagination** — the `--all` flag auto-paginates to fetch every
  result.
- **Custom Fields** — set priority, state, assignee, and any field with
  validation.
- **Comments & Links** — comment on issues and link them together.
- **Knowledge Base** — manage articles (YouTrack and Jira/Confluence).
- **AI-Optimized** — context aggregation, query templates, and workflow hints
  designed for coding agents.
- **Output Formats** — human-readable text and machine-readable JSON.
- **Flexible Config** — CLI flags, environment variables, or a config file.

## Next steps

- [Installation](/track-cli/installation/) — install with the native installer,
  Homebrew, Cargo, or a prebuilt binary.
- [Quick Start](/track-cli/quick-start/) — configure a backend and run your
  first commands.
- [Configuration](/track-cli/configuration/) — config files, environment
  variables, and backend selection.
- [Commands](/track-cli/commands/) — full command reference and aliases.

:::note
Documentation links use the `/track-cli/` base path because the site is served
from GitHub project pages. If a custom domain is configured later, that prefix
goes away.
:::
