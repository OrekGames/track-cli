---
title: Overview
description: track is a unified CLI for project issues and wikis across YouTrack, Jira, GitHub, GitLab, and Linear — one interface for humans and automation.
---

`track` is a unified command-line interface for project management tools and
wikis, built in Rust. It speaks to **YouTrack**, **Jira**, **GitHub**,
**GitLab**, and **Linear** through a single, unified set of commands — so the
way you get, create, search, and update issues and wiki articles is the same no
matter which backend your team uses.

## Why track

- **Multi-Backend** — five backends (project tools + wikis), one command set.
- **Issue Management** — get, create, update, delete, and search issues.
- **Batch Operations** — inspect, update, delete, or complete many issues at
  once, plus declarative bulk `apply`.
- **Transparent Pagination** — the `--all` flag auto-paginates to fetch every
  result.
- **Custom Fields** — set priority, state, assignee, and any field with
  validation.
- **Comments & Links** — comment on issues and link them together.
- **Wikis & docs** — manage articles and wiki pages (YouTrack Knowledge Base,
  Confluence, GitHub and GitLab wikis).
- **Capability Audit** — `track doctor` reports what each configured backend can
  actually do before you rely on it.
- **Rich context & workflows** — context aggregation, query templates, and
  workflow hints for humans and AI automation.
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
