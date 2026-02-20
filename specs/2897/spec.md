# Spec: Issue #2897 - session detail complete message coverage contracts

Status: Implemented

## Problem Statement
Existing session-detail tests verify panel/timeline markers and sampled rows, but do not explicitly enforce complete persisted message coverage across session detail lineage rendering. This leaves the PRD checklist item “Session detail shows all messages” incompletely proven.

## Scope
In scope:
- Add deterministic conformance tests for complete message coverage on session detail timeline markers.
- Ensure gateway detail rendering includes all persisted, non-empty lineage messages with deterministic role/content contracts.
- Preserve existing panel, graph, branch, and reset contracts.

Out of scope:
- Session reset/branch behavior changes.
- Memory graph behavior changes.
- New dependencies.

## Acceptance Criteria
### AC-1 Session detail timeline deterministically covers persisted non-empty lineage messages
Given a session with persisted lineage entries containing multiple message roles and distinct contents,
when `/ops/sessions/{session_key}` renders,
then timeline markers include each persisted non-empty message exactly once with deterministic role/content row contracts.

### AC-2 Timeline metadata reflects complete message count
Given persisted lineage entries in a selected session,
when session detail renders,
then `tau-ops-session-message-timeline` `data-entry-count` equals the number of rendered detail rows.

### AC-3 Existing detail contracts remain intact
Given existing detail contracts for validation/usage/graph,
when complete message coverage assertions are added,
then these existing contracts still pass unchanged.

### AC-4 Empty-message entries are not rendered as timeline rows
Given persisted entries with empty text content,
when session detail renders,
then empty-content entries are excluded from detail timeline rows while non-empty entries remain fully covered.

### AC-5 Regression safety for prior session/chat phases
Given suites `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2885`, `spec_2889`, and `spec_2893`,
when rerun,
then all suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Integration | persisted mixed-role lineage with distinct messages | render `/ops/sessions/{session_key}` | timeline rows include all non-empty persisted messages |
| C-02 | AC-2 | Functional | rendered detail timeline | inspect SSR markers | `data-entry-count` equals rendered row count |
| C-03 | AC-3 | Regression | existing detail route contracts | rerun detail suites | validation/usage/graph/detail contracts unchanged |
| C-04 | AC-4 | Integration | persisted empty + non-empty entries | render detail | empty entries excluded, non-empty fully present |
| C-05 | AC-5 | Regression | prior suites | rerun suites | no regressions |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_2897 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2897' -- --test-threads=1` passes.
- Required regression suite reruns remain green.
