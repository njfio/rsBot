# Spec: Issue #2866 - chat inline tool-result card contracts

Status: Reviewed

## Problem Statement
Tau Ops chat transcript includes tool-role messages as plain rows but does not expose explicit inline tool-result card markers. This prevents deterministic contract validation for the PRD requirement that tool use/results render inline in chat output.

## Scope
In scope:
- Add deterministic inline tool-result card marker elements for tool-role transcript rows.
- Keep non-tool row rendering behavior stable.
- Validate route-safe behavior on `/ops`, `/ops/chat`, and `/ops/sessions`.

Out of scope:
- Tool invocation pipeline changes.
- Message persistence schema changes.
- New API endpoints.

## Acceptance Criteria
### AC-1 Tool rows expose inline card markers
Given a chat transcript containing tool-role rows,
when shell markup renders,
then each tool row includes deterministic inline card marker attributes.

### AC-2 Non-tool rows remain non-card rows
Given a chat transcript containing non-tool rows,
when shell markup renders,
then non-tool rows do not expose inline tool-card markers.

### AC-3 `/ops/chat` route renders inline tool-result card contracts
Given `/ops/chat` request with a session containing tool output,
when shell renders,
then visible chat panel includes deterministic inline tool-card marker(s).

### AC-4 Non-chat routes preserve hidden-panel inline tool-card contracts
Given `/ops` and `/ops/sessions` requests with a session containing tool output,
when shell renders,
then chat panel remains hidden and deterministic inline tool-card markers remain present.

### AC-5 Regression safety for existing chat contracts
Given existing contract suites,
when `spec_2830`, `spec_2858`, and `spec_2862` rerun,
then all existing chat panel, visibility-state, and token-counter contracts remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | transcript includes tool row | render UI shell | tool row includes `tau-ops-chat-tool-card-*` marker |
| C-02 | AC-2 | Functional | transcript includes user/assistant rows | render UI shell | no tool-card marker for non-tool rows |
| C-03 | AC-3 | Integration | gateway `/ops/chat` request with tool message session | render response | chat panel visible and tool-card marker present |
| C-04 | AC-4 | Integration | gateway `/ops` and `/ops/sessions` requests with tool message session | render response | chat panel hidden and tool-card marker present |
| C-05 | AC-5 | Regression | existing chat suites | rerun suites | no regression in existing chat contracts |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2866 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2866' -- --test-threads=1` passes.
- `spec_2830`, `spec_2858`, and `spec_2862` suites remain green.
