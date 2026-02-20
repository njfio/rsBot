# Spec: Issue #2872 - chat new-session creation contracts

Status: Reviewed

## Problem Statement
Tau Ops chat currently supports selecting existing sessions and sending messages, but does not expose a deterministic contract for explicit “new session” creation flow. This leaves the PRD checklist item “New session creation works” unverifiable.

## Scope
In scope:
- Add additive SSR contract markers for chat new-session control/form.
- Add gateway route handling for new-session creation and redirect to `/ops/chat` with the created session key.
- Verify created session appears in selector contracts and hidden-route chat panel state.

Out of scope:
- Session branch/merge UX.
- Session lifecycle policy changes.
- New dependencies.

## Acceptance Criteria
### AC-1 Chat route exposes deterministic new-session control contracts
Given the `/ops/chat` shell,
when UI markup renders,
then deterministic new-session form markers (id/action/method/theme/sidebar/session fields) are present.

### AC-2 New-session submission creates and redirects to the new session
Given a valid new-session submission,
when `POST /ops/chat/new` is called,
then gateway creates/initializes the target session and returns redirect to `/ops/chat?...&session=<target>`.

### AC-3 Created session is selected in chat selector contracts
Given a newly created session,
when redirected chat route renders,
then session selector includes target session and marks it selected.

### AC-4 Non-chat routes preserve hidden chat panel contracts for created sessions
Given `/ops` or `/ops/sessions` requested with created session query,
when shell markup renders,
then chat panel remains hidden and still reflects created active session key in panel contracts.

### AC-5 Regression safety for existing chat contracts
Given existing chat contract suites,
when `spec_2830`, `spec_2834`, `spec_2858`, `spec_2862`, `spec_2866`, and `spec_2870` rerun,
then all suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops/chat` render | inspect shell markup | new-session form marker contract present |
| C-02 | AC-2 | Integration | valid post payload for new-session | call `POST /ops/chat/new` | redirect location includes created session key |
| C-03 | AC-3 | Integration | created session key | render redirected `/ops/chat` | selector contains/marks created session selected |
| C-04 | AC-4 | Integration | created session key on `/ops` + `/ops/sessions` | render route shells | chat panel hidden with created active session key |
| C-05 | AC-5 | Regression | existing chat suites | rerun suites | no regressions |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2872 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2872' -- --test-threads=1` passes.
- Required chat regression suites remain green.
