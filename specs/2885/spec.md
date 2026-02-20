# Spec: Issue #2885 - session branch creation and lineage contracts

Status: Implemented

## Problem Statement
Tau Ops sessions currently expose timeline/detail/graph contracts, but do not provide deterministic branch-action contracts to create a new session from a selected timeline message. This leaves the PRD checklist item “Branch creates a new session from selected message” unverifiable.

## Scope
In scope:
- Add deterministic session-branch form markers in the sessions detail timeline view.
- Add gateway session-branch endpoint contracts for branch creation and redirect behavior.
- Validate branched session lineage contains only entries through the selected message.
- Validate branched session becomes active in the redirected chat view.

Out of scope:
- Session merge workflows.
- Session reset workflows.
- Client-side JavaScript state management changes.
- New dependencies.

## Acceptance Criteria
### AC-1 Sessions detail route exposes deterministic branch-action form contracts
Given `/ops/sessions/{session_key}` with timeline rows,
when SSR markup renders,
then each timeline row exposes a deterministic branch form contract containing source session key, selected entry id, target session key input, and route state hidden fields.

### AC-2 Gateway branch action creates lineage-derived session and redirect contracts
Given a source session and selected entry id,
when `POST /ops/sessions/branch` is submitted,
then gateway creates/updates the target branch session with lineage entries up to the selected entry and returns a `303` redirect to `/ops/chat` with theme/sidebar/session query contracts.

### AC-3 Redirected chat route reflects branched session as active with branch-limited transcript
Given a successful branch action,
when redirected `/ops/chat` for the target branch session renders,
then chat selector and form session markers target the branch session and transcript excludes messages after the selected source entry.

### AC-4 Session lineage integrity remains valid after branch action
Given a successful branch action,
when branch session store validation is inspected,
then validation report is valid and lineage message content matches the selected prefix from source session.

### AC-5 Regression safety for prior sessions/chat contract phases
Given existing suites for `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2872`, and `spec_2881`,
when rerun,
then all suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | sessions detail with timeline rows | inspect SSR markup | row-level branch form markers and hidden contract fields present |
| C-02 | AC-2 | Integration | source session + selected entry id + target key | `POST /ops/sessions/branch` | `303` redirect to `/ops/chat?...&session=<target>` and branch session created |
| C-03 | AC-3 | Functional | successful branch session | render `/ops/chat` target | active session and transcript markers reflect branch-limited content |
| C-04 | AC-4 | Integration | successful branch action | load branch session store | validation is valid and lineage content equals selected prefix |
| C-05 | AC-5 | Regression | prior chat/session suites | rerun suites | no regressions |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2885 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2885' -- --test-threads=1` passes.
- Required chat/session regression suites remain green.
