# Spec: Issue #2846 - /ops/sessions/{session_key} session graph node/edge contracts

Status: Reviewed

## Problem Statement
Tau Ops now exposes deterministic session detail timeline/validation/usage contracts at `/ops/sessions/{session_key}`, but does not expose deterministic session graph contracts from lineage parent links. Operators cannot verify graph node and edge composition directly from SSR markers.

## Scope
In scope:
- Add deterministic SSR contracts for session graph panel on `/ops/sessions/{session_key}`.
- Render deterministic graph node row markers from selected session lineage entries.
- Render deterministic graph edge row markers from selected session parent links.
- Preserve existing `/ops/sessions` detail/list and `/ops/chat` contracts.

Out of scope:
- Interactive graph rendering/physics.
- Branch/merge/reset action execution.
- Any storage backend changes.

## Acceptance Criteria
### AC-1 Graph panel route contracts
Given a request to `/ops/sessions/{session_key}`,
when shell renders,
then the response includes deterministic graph panel marker attributes containing route and selected session key.

### AC-2 Graph node row contracts
Given selected session lineage entries,
when `/ops/sessions/{session_key}` renders,
then deterministic graph node row markers are present with entry-id and role attributes.

### AC-3 Graph edge row contracts
Given selected session lineage parent links,
when `/ops/sessions/{session_key}` renders,
then deterministic graph edge row markers are present with source and target entry-id attributes.

### AC-4 Graph summary counts
Given selected session lineage,
when `/ops/sessions/{session_key}` renders,
then graph list-level marker attributes expose deterministic node-count and edge-count values.

### AC-5 Graph empty fallback contracts
Given selected session with no entries,
when `/ops/sessions/{session_key}` renders,
then deterministic graph empty-state marker is present with node-count `0` and edge-count `0`.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | valid session detail route request | render shell | includes `tau-ops-session-graph-panel` marker with route + selected key |
| C-02 | AC-2 | Integration | selected session with lineage entries | render detail route | graph node row markers include deterministic ids + roles |
| C-03 | AC-3 | Integration | selected session with parent links | render detail route | graph edge row markers include deterministic source/target ids |
| C-04 | AC-4 | Functional | selected session lineage available | render detail route | graph summary marker counts match lineage-derived nodes/edges |
| C-05 | AC-5 | Functional | selected session has no entries | render detail route | graph empty-state marker present and counts are `0` |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui functional_spec_2846 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2846' -- --test-threads=1` passes.
- Regression suites for `spec_2846` dependencies (`spec_2842`, `spec_2838`, `spec_2834`) remain green.
