# #323: First-time `track init` error catalog

## Problem

First-time setup failures cross several layers: clap validates arguments,
`track init` validates and writes configuration, the GitHub client probes a
repository, and the first command reloads and validates the result. The current
messages do not consistently identify the failed phase, distinguish credentials
from connectivity or repository access, name the file involved in local I/O, or
say whether initialization already wrote a token-bearing config.

GitHub is the priority because its setup also requires choosing the non-default
backend and supplying `owner/repo`. A user can omit `--backend github`, create a
valid YouTrack-shaped config pointing at GitHub, and see a misleading YouTrack
error only on a later command.

Runtime failures are finally rendered by
`crates/track/src/main.rs::cli_main` through
`crates/track/src/output.rs::output_error` as `Error: {anyhow chain}`. Clap
argument failures are printed before `run` and do not use that wrapper.

## What “catalog” means

For this slice, the catalog is this documentation artifact:

- an inventory of first-time init and immediate post-init failures;
- the current user-visible message shape;
- the file and symbol that emit or wrap it;
- exact proposed copy or behavior for a focused follow-up; and
- a severity decision: **rewrite now** or **later child issue**.

This design PR changes no Rust, tests, command behavior, or error types.
“Rewrite now” below means suitable for the next focused implementation slice
using existing `anyhow`, `GitHubError`, and `TrackerError` paths. “Later child
issue” means the correction changes behavior, transaction boundaries, output
contracts, or multiple command families and should be separately scoped.

## Options considered

### 1. Documented inventory plus focused follow-ups — chosen

Keep the inventory in docs, identify a small copy-only/context-only tranche, and
split behavioral work into child issues. This satisfies the `type: docs` issue,
preserves the existing error model, and gives implementation owners exact emit
points and target text.

### 2. Rewrite all onboarding strings in one change

This would mix clap behavior, config validation, backend HTTP errors, filesystem
partial failure, and doctor output. It would be difficult to review and would
turn a catalog issue into a broad runtime refactor.

### 3. Add a structured diagnostic framework

Typed phases, paths, reasons, hints, and machine-readable codes could make the
messages uniform, but the repository has no shared path-and-reason diagnostic
wrapper to extend. A new framework is not justified by this issue.

## Chosen design

Use stable summaries followed by specific remediation:

1. Name the phase and backend: argument validation, repository validation,
   config write, gitignore update, config test, or first API call.
2. Name the relevant non-secret resource: config path, API base URL, or
   `owner/repo`.
3. Distinguish existing `GitHubError` variants instead of matching rendered
   strings: `Unauthorized`, `RateLimited`, `Api { status, .. }`, `Http`, and
   parse failures.
4. State write status when known. Probe failures happen before config creation;
   gitignore and guide failures happen after config creation.
5. Give one concrete next action. Never print, echo, classify by, or partially
   reveal the token.

No new public error type is required. Dynamic transport or server detail may
follow the stable summary as `<reason>`.

## Emit-path overview

- Argument parsing:
  `crates/track/src/cli.rs::Commands::Init` → clap output from
  `crates/track/src/main.rs::cli_main`.
- Init validation and writes:
  `crates/track/src/commands/init.rs::handle_init` →
  `validate_init_url`, `parse_github_project`, and
  `create_config_and_finish`.
- GitHub init probe:
  `handle_init` →
  `crates/github-backend/src/client.rs::GitHubClient::get_repo` →
  `GitHubClient::check_response` →
  `crates/github-backend/src/error.rs::GitHubError`.
- First command:
  `crates/track/src/main.rs::run` →
  `crates/track/src/config.rs::{Config::load, Config::validate}` →
  `build_client` → command handler.
- Connection diagnostics:
  `crates/track/src/commands/config.rs::handle_config` for `config test`,
  and `crates/track/src/commands/doctor.rs::{audit_backend,
  run_remote_checks, classify_error}` for `doctor`.

## Catalog

`<path>`, `<url>`, `<owner/repo>`, `<status>`, and `<reason>` are dynamic. The
leading runtime `Error:` renderer is omitted from proposed text.

| Failure | Current message | Emit path | Proposed after | Severity |
|---|---|---|---|---|
| Missing `--url` or `--token` | Clap: `error: the following required arguments were not provided:` followed by `--url <URL>` and/or `--token <TOKEN>`, usage, and `--help` hint. | `crates/track/src/cli.rs::Commands::Init` (`required_unless_present = "skills"`); clap exits from `Cli::parse`. | `Missing required setup arguments: --url and --token. GitHub example: track init --backend github --url https://api.github.com --token <TOKEN> --project owner/repo` | **Later child issue.** Replacing clap’s parser error is broader than a local string rewrite. Keep the command example in onboarding docs meanwhile. |
| GitHub URL supplied without selecting GitHub | There may be no error: `Init.backend` defaults to YouTrack and can write a YouTrack config containing the GitHub API URL. With `--project`, the YouTrack project probe can instead produce `Failed to connect to server or list projects: <reason>` and `Check your URL and token.` | `crates/track/src/cli.rs::Backend::default` and `Commands::Init.backend`; `crates/track/src/commands/init.rs::handle_init` backend match. | `The URL looks like GitHub, but --backend defaults to youtrack. Re-run with --backend github --project owner/repo. No config was written.` | **Later child issue.** This requires a backend-selection guard or an explicit-backend CLI change, not only new copy. |
| GitHub `--project` omitted | `GitHub init requires --project owner/repo` | `crates/track/src/commands/init.rs::handle_init`, `Backend::GitHub` branch. | `GitHub setup requires --project <owner>/<repo>. Example: --project OrekGames/track-cli. No config was written.` | **Rewrite now.** |
| GitHub project has no slash, an empty owner/repo, or extra slashes | `GitHub requires --project in owner/repo format, got '<value>'` | `crates/track/src/commands/init.rs::parse_github_project`. | `Invalid GitHub project '<value>': expected exactly <owner>/<repo>, for example OrekGames/track-cli. No config was written.` | **Rewrite now.** |
| URL is not absolute HTTP(S), has no host, contains userinfo, or uses remote plain HTTP | One of `Invalid URL: must be a valid absolute http:// or https:// URL`, `Invalid URL: must start with http:// or https://`, `Invalid URL: must include a host`, `Invalid URL: userinfo is not allowed in server URLs`, or the longer `Insecure URL: http:// is only allowed for local addresses ...` message. | `crates/track/src/commands/init.rs::validate_init_url`. | `Invalid --url '<url>': expected an absolute https:// URL with a host. Plain http:// is allowed only for localhost. GitHub.com uses https://api.github.com. No config was written.` | **Rewrite now.** Keep the localhost exception; consolidate parser-dependent wording. |
| GitHub web URL used as API base URL | Validation accepts `https://github.com`; the probe usually becomes `Failed to validate GitHub repository '<owner/repo>': API error (404): Not Found` followed by `Check your API URL, token, owner, and repo.` | `validate_init_url` → `handle_init` GitHub branch → `GitHubClient::{get_repo, check_response}`. | `'<url>' is a GitHub web URL, not an API base URL. Use https://api.github.com for GitHub.com or the API base URL supplied by your GitHub Enterprise administrator. No config was written.` | **Later child issue.** Enterprise URL rules and redirect behavior need separate, tested detection. |
| Target local or global config already exists | `Config file already exists: <path>` followed by `Use a text editor to modify it, or delete it first.` | `crates/track/src/commands/init.rs::handle_init`, `config_path.exists()` guard. | `Initialization stopped: config already exists at '<path>'. No files were changed. Update it with 'track config set' or remove it only if you intend to replace the configuration.` | **Rewrite now.** This removes an unnecessarily destructive default recommendation. |
| Config path cannot be resolved or global config directory cannot be created | `Failed to get current directory: <reason>`, `Could not determine home directory for global config`, or `Failed to create directory <path>: <reason>`. | `crates/track/src/config.rs::{local_track_config_path, global_config_path_ensure}`, called by `handle_init`. | `Could not choose the <project/global> config path: <reason>. No config was written. Run 'track config path' after fixing the current directory or home-directory environment.` | **Later child issue.** The fallback/action differs by platform and global versus local mode. |
| Config cannot be serialized, opened, or written | `Failed to serialize config: <reason>`, `Failed to open config file: <reason>`, or `Failed to write config file: <reason>`; open/write messages omit `<path>`. | `crates/track/src/commands/init.rs::create_config_and_finish` → `crates/track/src/config.rs::Config::save`. | `Could not create config '<path>': <reason>. Check directory permissions and available space. No later init files were written.` | **Later child issue.** `Config::save` is shared by non-init mutation commands; its path/reason contract should be handled as one focused config-I/O issue. |
| Existing `.gitignore` cannot be read or updated | Raw I/O text from `std::fs::read_to_string` or `std::fs::write`, without a stable operation, path, remedy, or notice that config already exists. | `crates/track/src/commands/init.rs::update_gitignore_if_present`, called after `Config::save` by `create_config_and_finish`. | `Config was created at '<config-path>', but '<gitignore-path>' could not be updated: <reason>. Add '.track.toml' and '.tracker-cache/' manually before committing.` | **Rewrite now.** This is security-relevant because the config can contain a token. Do not claim rollback. |
| Agent guide or optional skill installation fails after config creation | Guide and most skill directory/write failures are raw I/O text. `Cannot determine home directory` is the only stable skills-specific message. | `crates/track/src/commands/init.rs::{create_config_and_finish, install_agent_skills}`. | `Config was created at '<config-path>', but <artifact> could not be installed at '<path>': <reason>. Re-run 'track init --skills' for skills, or copy the guide manually.` | **Later child issue.** Multi-artifact retry and partial-success reporting deserve their own local-install issue. |
| GitHub probe returns 401 | `Failed to validate GitHub repository '<owner/repo>': Authentication failed` followed by `Check your API URL, token, owner, and repo.` | `handle_init` GitHub branch → `GitHubClient::get_repo` → `GitHubClient::check_response` → `GitHubError::Unauthorized`. | `GitHub authentication failed while validating '<owner/repo>'. Check that the token is valid and can read this repository. No config was written.` | **Rewrite now.** |
| GitHub probe returns 403 | `Failed to validate GitHub repository '<owner/repo>': API error (403): <server message>` followed by the same generic checklist. | Same probe path → `GitHubError::Api { status: 403, .. }`. | `GitHub denied access to '<owner/repo>' (403). Confirm the token can access the repository and, if applicable, is authorized for the organization. No config was written. GitHub said: <reason>` | **Rewrite now.** Do not assert one specific missing scope. |
| GitHub probe returns 404 | `Failed to validate GitHub repository '<owner/repo>': API error (404): <server message>` followed by the same generic checklist. | Same probe path → `GitHubError::Api { status: 404, .. }`. | `GitHub could not access repository '<owner/repo>' (404). Verify the owner/repo spelling and that the token can see a private repository. No config was written.` | **Rewrite now.** “Could not access” preserves GitHub’s intentional ambiguity between missing and private resources. |
| GitHub probe is rate-limited | `Failed to validate GitHub repository '<owner/repo>': Rate limited` followed by the generic checklist. | `GitHubClient::check_response` detects 403 plus `x-ratelimit-remaining: 0` and emits `GitHubError::RateLimited`. | `GitHub rate limiting prevented validation of '<owner/repo>'. Wait for the limit to reset or use a token with available quota, then retry. No config was written.` | **Rewrite now.** |
| DNS, connection, TLS, proxy, or timeout failure during the probe | `Failed to validate GitHub repository '<owner/repo>': HTTP error: <dynamic ureq reason>` followed by the generic checklist. | `GitHubClient::get_repo` request → `GitHubError::Http`; wrapped in `handle_init`. | `Could not reach the GitHub API at '<url>' while validating '<owner/repo>': <reason>. Check the API URL, network, proxy, and TLS setup, then retry. No config was written.` | **Rewrite now.** Preserve the source reason after a stable summary. |
| GitHub probe returns 5xx or an invalid success body | `API error (<status>): <server message>` for non-2xx, or a dynamic JSON/HTTP parse error for a malformed 2xx body, all under the generic init wrapper. | `GitHubClient::{check_response, get_repo}` → `GitHubError::{Api, Http}`; JSON read errors are converted by ureq. | For 5xx: `GitHub returned a server error (<status>) while validating '<owner/repo>'. Retry later. No config was written.` For malformed success: `The API at '<url>' returned an invalid GitHub repository response. Verify the API base URL or proxy. No config was written. Details: <reason>` | **Later child issue.** The current ureq conversion does not cleanly preserve a distinct response-parse variant at this call site. |
| First GitHub command has no token | `GitHub token not configured. Set via --token, TRACKER_TOKEN env var, or config file` | `crates/track/src/main.rs::run` → `crates/track/src/config.rs::Config::validate`. | `GitHub token is not configured. Set it with 'track config set github.token <TOKEN>', set GITHUB_TOKEN, or pass --token.` | **Rewrite now.** The current hint omits the supported GitHub-specific key and environment variable. |
| First GitHub command has no owner or repo | Separate messages: `GitHub owner not configured. Set via 'track config set github.owner <OWNER>' or GITHUB_OWNER env var` and the equivalent `repo`/`GITHUB_REPO` message. | `Config::validate`; defensive shorter forms also exist in `crates/track/src/main.rs::build_client`. | `GitHub repository is not fully configured: missing github.<owner/repo>. Set it with 'track config set github.<owner/repo> <VALUE>' or re-run 'track init --backend github --project owner/repo'.` | **Rewrite now.** Keep separate missing-field selection while using the same remedy shape. |
| `[github]` exists but effective backend is still YouTrack | A first command can report `YouTrack URL not configured...` even though GitHub owner, repo, and token are present. | `crates/track/src/config.rs::{resolve_backend, Config::apply_backend_config, Config::validate}`; dispatch in `main.rs::run`. | `GitHub settings were found, but the effective backend is youtrack. Set 'backend = "github"', run 'track config backend github', or pass '--backend github'.` | **Later child issue.** Detecting likely intended backend changes config-resolution behavior and must account for multi-backend configs. |
| `track -b github config test` authentication or access fails | Usually only `Authentication failed`, `API error (<status>): <message>`, or `HTTP error: <reason>` after the global `Error:` prefix. It does not identify GitHub or the probe. | `crates/track/src/commands/config.rs::handle_config`, `ConfigCommands::Test` → `IssueTracker::list_projects`; for GitHub, `crates/github-backend/src/trait_impl.rs::list_projects` → `GitHubClient::list_repos`. | `GitHub connection test failed while listing repositories: <reason>. Check github.token/GITHUB_TOKEN and run 'track -b github doctor' for per-check details.` | **Rewrite now.** Add command/backend context around the existing error; do not change the probe in this tranche. |
| `track doctor` finds bad GitHub auth, access, or connectivity | Structured checks already show `config_valid`, `auth_connectivity`, status, and the underlying `TrackerError`; 403 adds `token likely lacks the required scope`. `--strict` additionally prints only aggregate counts. | `crates/track/src/commands/doctor.rs::{audit_backend, run_remote_checks, classify_error}`; GitHub errors map in `crates/github-backend/src/error.rs::From<GitHubError> for TrackerError` and `crates/tracker-core/src/error.rs::TrackerError`. | Retain doctor as the detailed diagnostic path. A future child issue may make its 401/403/404 remedies match the init catalog without changing check/status semantics. | **Later child issue.** Doctor is already structured and covers all backends. |
| First GitHub issue request fails after client creation | Command context plus a generic cause, for example `Failed to fetch issue '1': Authentication failed`; search and create use their own operation wrappers. | `crates/track/src/commands/issue.rs` handlers → `GitHubClient` → `GitHubError` → `TrackerError`; final rendering through `output_error`. | `Failed to fetch GitHub issue '1': authentication failed. Check github.token/GITHUB_TOKEN or run 'track -b github doctor'.` Apply the same phase/backend/remedy shape to other first operations. | **Later child issue.** Rewriting every command wrapper is outside the init catalog and should be a cross-command consistency issue. |

## Immediate implementation boundary

The **rewrite now** tranche is intentionally limited to:

- copy in `commands/init.rs` for missing/malformed GitHub project, invalid URL,
  existing config, and post-config gitignore failure;
- classification of the existing `GitHubError` variants at the init repository
  probe, with no new framework and no string matching;
- GitHub-specific missing token/owner/repo hints in `Config::validate`; and
- one context wrapper around the existing `config test` probe.

If that set is still too broad for one implementation PR, split it at the
network boundary: init argument/filesystem copy first, then GitHub probe and
first-command copy. Do not absorb any **later child issue** merely because its
emit point is nearby.

## Out of scope

- Production Rust changes, tests, or refactors in this design PR.
- A new diagnostic/error framework, public error codes, or a Pummel-style
  path-and-reason type.
- Changing which GitHub endpoint init, `config test`, or doctor probes.
- Automatic backend inference, token validation, OAuth/login flows, or GitHub
  Enterprise discovery.
- Transactional init, rollback, overwrite/force behavior, or changes to file
  creation order and permissions.
- Creating `.gitignore` when absent or changing which entries are added.
- JSON error-output changes. `cli_main` currently calls `output_error` with text
  format even when `-o json` was requested; that is a separate output-contract
  issue.
- A message sweep for Jira, YouTrack, GitLab, or Linear.
- Redesigning doctor status classification or rewriting all command-level
  `anyhow::Context` strings.
- Closing #323 or treating this catalog as a fix for runtime behavior.

## Risks and open questions

- GitHub deliberately uses 404 for both missing and inaccessible private
  repositories. Proposed text must not claim which one occurred.
- A 403 can mean token permission, organization authorization, policy, or abuse
  throttling. Only the explicit rate-limit header supports a rate-limit claim.
- `ureq` transport and body-decode details are dynamic. Tests should assert the
  stable summary, not platform-specific `<reason>` text.
- Init validates the configured repository with `GET /repos/{owner}/{repo}`,
  while GitHub `config test` and doctor’s `auth_connectivity` use
  `GET /user/repos`. Their results can legitimately differ; copy must identify
  the probe rather than claim universal connectivity.
- Local init writes `.track.toml` before touching `.gitignore` and
  `AGENT_GUIDE.md`. A later failure is partial success, not “no files written.”
- A GitHub.com web-URL hint is straightforward, but GitHub Enterprise API base
  URLs vary. That detection needs examples and test fixtures before design is
  finalized in a child issue.
- Replacing clap’s missing-argument output may affect shell scripts and help
  snapshots; onboarding docs are the safer immediate home for the full GitHub
  command.

## Done when

Code Optimizer can proceed without another design decision when:

- every selected rewrite uses the exact phase/resource/remedy shape above;
- existing `GitHubError` variants, not rendered strings, select 401, 403, 404,
  rate-limit, and transport copy;
- messages accurately state whether config was not written, already existed, or
  was written before an ancillary failure;
- token values never appear in errors or test fixtures;
- the rewrite-now boundary is kept separate from child-issue behavior; and
- follow-up work references #323 with `Relates to #323`, never
  `Fixes #323` or `Closes #323`.
