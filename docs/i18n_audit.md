# Internationalization / Unicode Audit

**Audit date:** 2026-05-10 (codebase at `main` @ `62585b4`)
**Scope:** All production crates (`tracker-core`, `youtrack-backend`, `jira-backend`, `github-backend`, `gitlab-backend`, `track`, `tracker-mock`, `agent-harness`).
**Status:** Catalog only — no code changes have been made.

## Background

Triggered by review of PR #236 (`⚡ Bolt: Optimize Jira transition lookup with eq_ignore_ascii_case`),
which replaced `String::to_lowercase()` with `str::eq_ignore_ascii_case()` in `resolve_transition_id`.
The change is a perf optimization for ASCII workflows but narrows case-insensitive matching to ASCII
A–Z ↔ a–z only, breaking it for non-English Jira workflows (German `Geöffnet`, French `À faire`,
Cyrillic `Открыто`, Czech `Otevřeno`, etc.) when input case differs from the workflow definition.

This audit extends that analysis to the whole codebase to identify other places where
non-English user data or API data is handled in ways that may fail.

## Severity definitions

- **HIGH** — Silent functional breakage of valid non-English input. User issues a correct command
  and it produces wrong or empty results, or panics.
- **MEDIUM** — UX degradation, internal inconsistency, or crash on rare inputs.
- **LOW** — Confirmed safe after review, listed for completeness so future contributors
  don't re-audit them.

## HIGH — ASCII-only case fold against API-supplied (potentially localized) data

Same class as the PR #236 site. Right-hand side is data returned by the tracker and can be
non-English in real deployments. Users typing the value in a different case won't match.

| File:Line | Function / context | Comparison |
|---|---|---|
| `crates/jira-backend/src/client.rs:586,590` | `resolve_transition_id` | user status name vs Jira workflow status name (PR #236 — revert recommended) |
| `crates/track/src/output.rs:232-237` | `find_custom_field_by_name` | user-requested field name vs API custom-field name |
| `crates/track/src/commands/issue.rs:1619-1624` | duplicate of above pattern | same |
| `crates/track/src/commands/issue.rs:540,621` | project-fields lookup | user field name vs project schema field name |
| `crates/track/src/commands/issue.rs:637` | enum-value validator | user value vs API enum bundle values (e.g., localized priorities) |
| `crates/track/src/commands/init.rs:261-263` | `track init` project picker | user input vs project `name` (full name can be non-ASCII, e.g. `Projet Démo`) |
| `crates/track/src/commands/config.rs:716-718` | config project lookup | same as init |
| `crates/youtrack-backend/src/client.rs:698` | link-type-by-name resolution | user link-type vs admin-defined link type name |
| `crates/track/src/cache.rs:1096,1190-1191,1201` | tag / query-template / link-type by name | user-defined names — risk depends on whether users create non-ASCII names |

**Common failure shape:**

| User input | API value | `to_lowercase` (correct) | `eq_ignore_ascii_case` (current) |
|---|---|---|---|
| `"in progress"` | `"In Progress"` | match | match |
| `"geöffnet"` | `"Geöffnet"` | match | match (only ASCII letters fold) |
| `"GEÖFFNET"` | `"Geöffnet"` | match | **fails** (`Ö` ≠ `ö` byte-wise) |
| `"à faire"` | `"À faire"` | match | **fails** |
| `"открыто"` | `"Открыто"` | match | **fails** |
| `"完了"` | `"完了"` | match | match (CJK has no case) |

Lowercase-or-title-case input still works for most accented languages. **All-caps input with
accents fails.** CJK / Arabic / Hebrew / Korean unaffected (those scripts have no case concept).

## HIGH — `to_lowercase().contains(<English keyword>)` against API data

UX-only when used for styling, but a real regression for non-English instances.

| File:Line | Behavior | Failure mode |
|---|---|---|
| `crates/track/src/output.rs:397` | Color status yellow if `val.to_lowercase().contains("progress")` | "En cours" (FR), "В работе" (RU), "対応中" (JA) render without the in-progress yellow highlight |

## HIGH — Slug generation drops all non-ASCII characters

| File:Line | Behavior | Failure mode |
|---|---|---|
| `crates/github-backend/src/trait_impl.rs:350-361` | `slugify()` lowercases then keeps only `is_ascii_alphanumeric`, replacing the rest with `-` | A wiki article titled `"はじめに"` slugifies to `"----"`; `"Café au lait"` → `"caf--au-lait"`; multiple distinct non-English titles can collapse to the same slug |

## HIGH — Byte-index slicing into strings — panics on non-ASCII

`&s[..N]` panics with `byte index N is not a char boundary` when byte N falls inside a
multi-byte UTF-8 sequence.

| File:Line | Code | Risk |
|---|---|---|
| `crates/agent-harness/src/runner.rs:191` | `format!("{}...", &output[..500])` | Panics on subprocess output where byte 500 is mid-UTF-8 |
| `crates/agent-harness/src/claude_code.rs:419` | `format!("{}...", &content[..300])` | Same |
| `crates/agent-harness/src/gemini_runner.rs:411` | `format!("{}...", &content[..300])` | Same |

`agent-harness` is internal evaluation tooling, so blast radius is limited, but it's a real
crash bug. Fix with `char_indices()` or `str::floor_char_boundary` (stable since Rust 1.80).

## MEDIUM — Inconsistent case-fold inside one module

`crates/track/src/output.rs` uses **two different folds** on the same data (custom-field name):

| Line | Fold used |
|---|---|
| 91, 94, 97, 107, 112, 114, 117, 127, 179, 193 | `to_lowercase()` (Unicode) — insertion + lookup into `requested_fields` / `displayed_fields` |
| 232-237 | `eq_ignore_ascii_case` — `find_custom_field_by_name` |

Same input can be considered "requested" by one path and "not found" by the other, marking
real fields as `Ignored` in the diff output for non-ASCII custom-field names that differ in
case from the user's input.

## MEDIUM — Localized hashtag queries

`#open` / `#unresolved` / `#inprogress` are matched case-insensitively against hard-coded
English keywords. Not breakage — falls back to passing the unrecognized hashtag through —
but there's no path for a non-English user to use convenient `#offen` or `#abierto` shorthands.

| File:Line | Backend |
|---|---|
| `crates/jira-backend/src/trait_impl.rs:330` | Jira `#hashtag` → JQL converter |
| `crates/gitlab-backend/src/convert.rs:320` | GitLab `#hashtag` → state filter |

## LOW — Locale-affecting matches against hard-coded English (confirmed safe)

The right-hand side is a fixed English keyword by design. Non-English input not matching
the literal is the intended behavior. Listed so future contributors don't re-audit:

- `crates/jira-backend/src/convert.rs:372,387,434` — `"priority"`, `"type"`, `"issuetype"`
- `crates/github-backend/src/convert.rs:124,154-156,173` — `"assignee"`, `"status"`, `"state"`, `"stage"`
- `crates/gitlab-backend/src/trait_impl.rs:142-144` — same set
- `crates/track/src/output.rs:421` — `"priority"`
- `crates/track/src/commands/issue.rs:1247,1440` — `"State"` / `"Stage"`
- `crates/track/src/commands/context.rs:71` — `"priority"`
- `crates/tracker-core/src/models.rs:397,467` — `CustomFieldType::parse` / `BundleType::parse` (CLI tokens)
- `crates/track/src/cache.rs:1284` — `parse_duration` (`h`/`m`/`s`/`d` suffixes)
- `crates/youtrack-backend/src/client.rs:700` — direction `"Inward"`/`"Outward"` (protocol)
- `crates/tracker-mock/src/scenario.rs:228` — backend name (`"youtrack"`/`"jira"` — protocol)
- `crates/track/src/commands/issue.rs:1119,1172` — `"subtask"`, `"parent"`, `"relates"`, `"depends"` (CLI tokens)

## LOW — Project short-name comparisons (confirmed safe)

Project short names are uppercase ASCII identifiers by tracker convention (e.g., `PROJ`, `SMS`).
Real-world risk of non-ASCII project short names is essentially zero.

- `crates/track/src/cache.rs:703,790,1078,1087,1105`
- `crates/track/src/commands/context.rs:166-181`
- `crates/youtrack-backend/src/client.rs:478`
- `crates/tracker-mock/src/client.rs:318-320`

## LOW — Already Unicode-correct (confirmed safe)

These use `to_lowercase()` consistently on both sides of the comparison, so Unicode case
folding works correctly for non-ASCII content:

- `crates/github-backend/src/wiki.rs:790,795-800` — wiki search
- `crates/gitlab-backend/src/trait_impl.rs:471-481` — wiki search
- `crates/tracker-mock/src/evaluator.rs:316-337` — scenario evaluator

## Recommended triage order

1. **PR #236** — already under revert review (`crates/jira-backend/src/client.rs:586,590`).
2. **Byte-index slicing in `agent-harness`** (HIGH, crash risk) — three sites, trivial fix
   with `char_indices` or `str::floor_char_boundary`.
3. **Custom-field name resolution** (HIGH, functional) — `track/src/output.rs:232-237`,
   `track/src/commands/issue.rs:1619-1624`, plus the matching `find` calls at
   `issue.rs:540,621,637`. Most commonly hit accessibility regression — every `--field`
   operation goes through this path.
4. **Slug generation** (HIGH, data corruption) — `github-backend/src/trait_impl.rs:350-361`.
   Wiki feature is limited but non-ASCII titles silently degrade.
5. **Status-color heuristic** (MEDIUM, UX) — `track/src/output.rs:397`.
6. **`output.rs` internal inconsistency** (MEDIUM) — pick one fold, audit nearby code.
7. **Project name lookup** (`init.rs`, `config.rs`) — lower priority since users typically
   pick projects by short_name (ASCII), but a non-ASCII full-name search would silently miss.

## Fix patterns

### Replacing `eq_ignore_ascii_case` against API data

When the right-hand side is API-supplied user-facing data, prefer one of:

```rust
// Option 1: revert to to_lowercase (allocates, but Unicode-correct)
a.to_lowercase() == b.to_lowercase()

// Option 2: allocation-free Unicode-aware comparison
fn unicode_eq_ignore_case(a: &str, b: &str) -> bool {
    let mut ai = a.chars().flat_map(char::to_lowercase);
    let mut bi = b.chars().flat_map(char::to_lowercase);
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if x == y => continue,
            _ => return false,
        }
    }
}
```

### Replacing `&s[..N]` byte slicing

```rust
// Wrong — panics on non-ASCII boundary
&s[..N]

// Right — char-aware truncation
let cut = s.char_indices().nth(N).map(|(i, _)| i).unwrap_or(s.len());
&s[..cut]

// Or, stable since 1.80:
&s[..s.floor_char_boundary(N)]
```

### Replacing slug generation that drops non-ASCII

Use the `unicode-normalization` + `deunicode` crates, or accept non-ASCII chars in slugs
(modern URL/filesystem layers handle UTF-8 fine). The current `is_ascii_alphanumeric`
filter is overly conservative.
