# Implementation Plan: Jira Backend State Transition Bug (Issue #211)

## Objective
Fix the `400` error that occurs when transitioning a Jira issue's state by explicitly resolving "state" to "status", implementing the `/transitions` API endpoint, and dynamically caching project workflow statuses.

## Background & Motivation
Currently, attempting to update a Jira issue's state (e.g., `track issue update PROJ-123 --state "Backlog"`) results in a 400 error if the project has a custom field named "State". The CLI incorrectly maps the update to the custom field instead of the built-in status workflow. Even if it mapped correctly, the Jira API requires workflow state changes to use the `POST /rest/api/3/issue/{key}/transitions` endpoint, which is currently unimplemented (the CLI attempts a `PUT`). Additionally, the local cache hardcodes statuses, preventing validation of actual workflow states.

## Scope & Impact
- **Scope:** Restricted to the `jira-backend` crate. No changes required in `tracker-core` or other backends. 
- **Impact:** Fixes a critical bug preventing state transitions for Jira issues via the CLI. Improves caching accuracy for Jira workflows.

## Proposed Solution
1. **Fix 1:** Add `"state"` as an alias to the reserved fields list in `resolve_extra_fields` to prevent collisions with custom fields.
2. **Fix 2:** Implement models and client methods for the `/transitions` endpoint. Update `update_issue` in `trait_impl.rs` to extract the state update and apply it via `POST /transitions` after `PUT`ting other field updates.
3. **Fix 3:** Refactor cache refresh logic to fetch real, per-project workflow statuses via `GET /rest/api/3/project/{projectIdOrKey}/statuses` instead of using hardcoded defaults.

## Implementation Plan

### Phase 1: Skip-list Hardening
- **File:** `crates/jira-backend/src/convert.rs`
- Factor the reserved fields list into a constant `RESERVED_FIELD_NAMES` including `"state"`.
- Update `resolve_extra_fields` to use `is_reserved_field(name)`.
- Add unit tests.

### Phase 2: Transitions Endpoint
- **Models:** Create `crates/jira-backend/src/models/transitions.rs` for `TransitionsResponse`, `Transition`, `TransitionTarget`, etc.
- **Client:** Update `crates/jira-backend/src/client.rs` to add `list_transitions`, `transition_issue`, and `resolve_transition_id`.
- **Trait Implementation:** Update `update_issue` in `crates/jira-backend/src/trait_impl.rs` to separate state updates and apply them via the transitions endpoint.
- **Error Handling:** Add `InvalidTransition` variant to `JiraError`.
- **Tests:** Add wiremock tests in `client_tests.rs`.

### Phase 3: Dynamic Status Cache
- **Client & Models:** Add models for project statuses and `list_project_statuses` to `client.rs`.
- **Conversion:** Add `flatten_project_statuses` in `convert.rs` to deduplicate and map statuses.
- **Cache Refresh:** Splicing fetched statuses into the `"status"` entry in `get_standard_custom_fields()`.
- **Tests:** Add tests for deduplication and cache population.

## Verification & Testing
- Run existing unit and integration tests for `jira-backend`.
- Ensure new wiremock tests for `update_issue` transitions pass.
- Verify end-to-end `track issue update PROJ-123 --state "Target State"` functions without returning a 400 error.
- Verify `track cache refresh` pulls the actual custom statuses for a Jira project.

## Migration & Rollback
No database migration needed. Projects relying on a literal custom field named "State" instead of the built-in Jira workflow state will see behavior change. If critical regressions occur, the changes can be easily reverted by rolling back the git commit.
