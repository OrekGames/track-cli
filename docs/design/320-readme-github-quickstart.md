# #320: README GitHub quick start

## Problem

The README names GitHub as a supported backend, but its Quick Start initialization
examples cover only YouTrack, Jira, and Linear. A first-time GitHub user must
currently assemble the setup path from the environment-variable list, backend
notes, and website docs. That path has three easy-to-miss requirements:

- GitHub is not the default backend, so init must include `--backend github`.
- GitHub.com init needs the API base URL `https://api.github.com`, a token, and
  the repository in `owner/repo` form through `--project`.
- After init, issue IDs and searches are repository-scoped, so the first useful
  check is a GitHub search such as `track i s "is:open"`.

This is a README copy specification. It does not introduce a new onboarding
command or change production code, tests, flags, parsing, or runtime behavior.

## Options considered

### 1. One focused GitHub subsection in Quick Start — chosen

Add one short subsection containing token preparation, the complete current
`track init` command, the first search, and links to the existing reference
docs. This keeps the successful path together and makes the required backend,
API URL, and repository shape visible without duplicating the backend reference.

### 2. Add only a GitHub init line to the existing backend examples

This is shorter, but it leaves token preparation and the first successful
command elsewhere. It would not provide the requested copy-pasteable first-time
path.

### 3. Copy the full GitHub backend reference into the README

This would make the Quick Start long and create a second source of truth for
configuration methods, identifiers, capabilities, and limits. Those details
already belong in the website's backend and configuration pages.

## Chosen design

Under README `## Quick Start` → `### 1. Initialize Configuration`, insert one
`#### GitHub` subsection after the existing paragraph that warns that local
config can contain tokens and before the current YouTrack/Jira/Linear command
examples. Keep the commands in one block and preserve the order shown below.

Use explicit GitHub.com values that work today. Do not rely on backend
inference, a default API URL, or environment-only configuration in this path.
After the two documented placeholders are replaced, the block is ready to
paste into a POSIX-style shell.

## Required README content

Use this copy:

````markdown
#### GitHub

Create a [GitHub personal access token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens)
that can access issues in the target repository. Replace
`YOUR_GITHUB_TOKEN` and `owner/repo` below; `owner/repo` is the repository
path, for example `OrekGames/track-cli`.

```bash
track init \
  --backend github \
  --url https://api.github.com \
  --token YOUR_GITHUB_TOKEN \
  --project owner/repo

track i s "is:open"
```

`track init` validates the repository and writes a local `.track.toml`; the
next command searches open issues using that saved GitHub configuration. See
the [GitHub backend notes](https://orekgames.github.io/track-cli/backends/#github)
and [configuration reference](https://orekgames.github.io/track-cli/configuration/)
for backend behavior and other configuration methods.
````

The placeholders are intentionally shell-safe:

- `YOUR_GITHUB_TOKEN` means the token value, not an environment-variable name
  that the reader must define.
- `owner/repo` means the exact GitHub repository path, without a protocol,
  hostname, issue number, or `.git` suffix.

The first search deliberately omits `--backend`, `--url`, `--token`, and
`--project`: successful init saves the selected backend and repository
configuration. `track i s` is the existing alias for `track issue search`, and
the GitHub query adapter scopes the search to the configured repository and
filters results to issues.

The two required reference links are the published website pages above. There
is no `CONTRIBUTING` file on `main` at design time, so this change must not add
a broken `CONTRIBUTING.md` link. If a contribution guide exists on the
implementation branch, it may be linked separately, but it must not replace
the setup-reference links or expand this block.

## What not to invent / out of scope

- No new onboarding, login, token-prompt, or config-generation command.
- No flags other than the real current init flags shown above. In particular,
  do not invent `--owner`, `--repo`, `--github-token`, or an interactive mode;
  repository selection is `--project owner/repo`.
- No undocumented environment variables, secret managers, `gh auth` handoff,
  device flow, OAuth flow, or browser authorization flow.
- No specific classic-token scope or fine-grained permission recipe. GitHub
  token models differ; link to GitHub's maintained token documentation and
  state only the required outcome: access to issues in the target repository.
- No backend auto-detection. GitHub must remain explicit with
  `--backend github`.
- No web-to-API URL rewrite or claim that `https://github.com` is accepted,
  detected, or corrected. The GitHub.com path must state
  `--url https://api.github.com`.
- No clap/error-message rewrite, replacement of missing-argument behavior, or
  documentation of proposed runtime copy from #323.
- No GitHub Enterprise URL discovery or guessed Enterprise URL shape.
- No full config-file sample, environment-variable matrix, backend capability
  table, token-permission matrix, or command reference in the README block;
  link to the existing docs instead.
- No production code, tests, CLI behavior changes, issue closure, or claim that
  this documentation fixes #320.

These boundaries match `docs/design/323-init-error-catalog.md`: the README
shows the complete rewrite-now command
`track init --backend github --url https://api.github.com --token <TOKEN> --project owner/repo`
without documenting that design's later-child backend inference,
web-versus-API URL handling, or clap changes.

## Risks / open questions

- Passing a token through `--token` can place it in shell history or a process
  listing. The current init command requires this flag for the documented path;
  designing a safer prompt or credential handoff is a separate CLI change.
- GitHub token types and permission names can change. The README should avoid
  a scope list and retain the authoritative GitHub token-documentation link.
- The command targets GitHub.com. Enterprise installations require an
  administrator-supplied API base URL and are intentionally not covered by this
  first-time block.
- Init validates `owner/repo` before writing configuration, so a token without
  repository access will stop at init. The copy must not promise that merely
  creating any token is sufficient.
- `CONTRIBUTING` is absent on `main`. Adding a speculative link would fail the
  acceptance goal rather than improve navigation.

## Done when

Code Optimizer can edit the README without another design question when:

- exactly one short GitHub subsection appears at the specified Quick Start
  location;
- its command sequence, order, placeholders, explanatory copy, and two
  reference links match this specification;
- the init command explicitly includes `--backend github`,
  `--url https://api.github.com`, `--token YOUR_GITHUB_TOKEN`, and
  `--project owner/repo`;
- the first post-init command is exactly `track i s "is:open"`;
- no nonexistent `CONTRIBUTING` link or unimplemented flag, environment
  variable, OAuth flow, backend detection, URL rewrite, or clap behavior is
  documented; and
- the implementation work relates to #320 without fixing, closing, or merging
  the issue as part of this design slice.
