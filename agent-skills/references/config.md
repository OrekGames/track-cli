# Configuration, doctor and setup

Read when a command fails for configuration reasons, or to set up or audit a backend.

## Configuration

**Preferred**: Use a `.track.toml` config file (created by `track init` or manually). Avoid passing `--url`/`--token` as CLI flags — those are a last resort for one-off commands.

### Config File (`.track.toml`)

Place in your project directory (or `~/.tracker-cli/.track.toml` for global defaults):

**YouTrack**:
```toml
backend = "youtrack"
url = "https://youtrack.example.com"
token = "perm:xxx"
default_project = "PROJ"
```

**Jira**:
```toml
backend = "jira"
url = "https://your-domain.atlassian.net"
email = "you@example.com"
token = "your-api-token"
```

**GitHub**:
```toml
backend = "github"

[github]
token = "ghp_xxxxxxxxxxxx"
owner = "your-org"
repo = "your-repo"
# api_url = "https://api.github.com"  # default, omit unless GHE
```

**GitLab**:
```toml
backend = "gitlab"

[gitlab]
token = "glpat-xxxxxxxxxxxx"
url = "https://gitlab.com/api/v4"
project_id = "12345"
# namespace = "your-group"
```

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

### Quick Setup with `track init`

`track init` creates a `.track.toml` in the current directory (or globally with `-g`/`--global`). **GitHub and GitLab require `--project`**:

```bash
track init --url https://youtrack.example.com --token YOUR_TOKEN --project PROJ
track init --url https://company.atlassian.net --token API_TOKEN --backend jira --email you@example.com
track init --url https://api.github.com --token ghp_TOKEN --backend github --project your-org/your-repo
track init --url https://gitlab.com/api/v4 --token glpat-TOKEN --backend gitlab --project 12345
track init --url https://linear.app/your-workspace --token lin_api_TOKEN --backend linear --project ORE
```

Note: `init` does NOT set `default_project` for GitHub/GitLab (scope comes from `owner`/`repo` / `project_id`), but `issue create` still requires `-p` or `default_project` — see Issue Operations notes.

### Install Agent Skills

```bash
track agent install    # Skills for Claude Code, Copilot, Cursor and Gemini CLI, plus the Claude Code session hook
track init --skills    # The skills only
```

Can be combined: `track init --url ... --token ... --skills`

Installed destinations:

| Agent | Path |
|-------|------|
| Claude Code | `~/.claude/skills/track/` (`SKILL.md` plus `references/`) |
| GitHub Copilot | `~/.copilot/skills/track/` (`SKILL.md` plus `references/`) |
| Cursor | `~/.cursor/skills/track/` (`SKILL.md` plus `references/`) |
| Gemini CLI | `~/.gemini/skills/track/` (`SKILL.md` plus `references/`) |

### Environment Variables (alternative)

| Backend | Variables |
|---------|-----------|
| Any backend | `TRACKER_BACKEND`, `TRACKER_URL`, `TRACKER_TOKEN`, `TRACKER_CONFIG` (alt config path), `TRACKER_<KEY>` (any config key, e.g. `TRACKER_DEFAULT_PROJECT`) |
| YouTrack | `YOUTRACK_URL`, `YOUTRACK_TOKEN` |
| Jira | `JIRA_URL`, `JIRA_EMAIL`, `JIRA_TOKEN` |
| GitHub | `GITHUB_TOKEN`, `GITHUB_OWNER`, `GITHUB_REPO`, `GITHUB_API_URL` |
| GitLab | `GITLAB_TOKEN`, `GITLAB_URL`, `GITLAB_PROJECT_ID`, `GITLAB_NAMESPACE` |
| Linear | `LINEAR_TOKEN`, `LINEAR_API_URL`, `LINEAR_URL`, `LINEAR_DEFAULT_TEAM`, `LINEAR_DEFAULT_PROJECT` |

**Priority** (highest wins): CLI flags → env vars → local `.track.toml` → global `~/.tracker-cli/.track.toml`. Note `TRACKER_URL`/`TRACKER_TOKEN` bind to the global `--url`/`--token` flags, so they get CLI-flag-level priority.

**Repository configs cannot redirect your token.** If the current directory's `.track.toml` sets a backend URL (or `github.api_url` / `linear.api_url`) but not its token, commands fail with `Refusing to send your <backend> token to <host>` instead of sending the token from your global config, environment, or `--token` there. Do **not** run `track config trust` yourself to get past this: it lets that repository receive the user's credentials, so ask the user. To keep working with the user's own setup, pass `--config ~/.tracker-cli/.track.toml` (or set `TRACKER_CONFIG`), which ignores the repository file.

Every command also accepts `--config <PATH>` to point at an alternate TOML file:
```bash
track --config ./ops/.track.jira.toml i s "project = OPS"
```

### Config Management

```bash
track config show              # Show current config
track config test              # Test connection (needs API; everything else below is offline)
track config keys              # List all config keys
track config backend github    # Switch default backend
track config set <key> <value> # Set a value (-g writes the global file)
track config get <key>         # Get a value
track config clear [-g]        # Delete a config file
track config trust             # Let this directory's .track.toml receive your token (user decision only)
track config untrust           # Revoke that trust
```

**Config keys** (settable via `config set`): `backend`, `url`, `token`, `email`, `default_project`, `youtrack.url`, `youtrack.token`, `jira.url`, `jira.email`, `jira.token`, `github.token`, `github.owner`, `github.repo`, `github.api_url`, `gitlab.token`, `gitlab.url`, `gitlab.project_id`, `gitlab.namespace`, `linear.token`, `linear.api_url`, `linear.url`, `linear.default_team`, `linear.default_linear_project`

**Notes**:
- `link_mappings` tables (`[youtrack.link_mappings]`, `[jira.link_mappings]`, `[gitlab.link_mappings]`, `[linear.link_mappings]`) are **config-file-only** — `config set/get` rejects them; edit `.track.toml` directly.
- Secret keys are **redacted on read**: `config get token` prints `(set - hidden)`. You can verify a token is set but never read it back.

### Doctor (Capability Audit)

```bash
track doctor -o json                 # Audit the effective backend
track doctor --all-backends -o json  # Audit every backend configured in .track.toml/global/env
track -b gitlab doctor -o json       # Audit a specific backend (global -b flag)
track doctor --project PROJ         # Use PROJ for project-scoped checks
track doctor --write-check          # + local write-payload validation (never mutates remote trackers)
track doctor --all-backends --strict # Exit non-zero if any check or backend failed (degraded still exits 0)
```

**`config test` vs `doctor`**: `config test` is a single connectivity probe (`list_projects`). `doctor` reports per-check capability statuses — `config_valid`, `auth_connectivity`, `project_resolution`, `issue_search`, `issue_read`, `comments_read`, `links_read`, `field_schema`, `field_admin`, `articles`, `write_validation` — each `ok`/`degraded`/`failed`/`skipped`. `degraded` usually means a scope-limited token (403); `skipped` means the backend doesn't support the capability. A backend with failing checks but working search/read is reported `degraded`, not `failed`; it rolls up `failed` when nothing practical works (bad credentials, or a broken read path — e.g. every call 404s under a wrong project id). JSON shape: `{summary: {backends_checked, ok, degraded, failed}, backends: [{backend, status, config, checks: [{name, status, message?, sample_count?}], recommendation?}]}`. Run `track doctor --all-backends -o json` at session start to learn which backends are trustworthy before drawing conclusions from individual command failures.
