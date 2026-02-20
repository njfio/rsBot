# Spec: Issue #2842 - /ops/sessions/{session_key} detail timeline/validation/usage contracts

Status: Reviewed

## Problem Statement
Tau Ops currently exposes deterministic `/ops/sessions` list contracts, but does not expose a deterministic detail contract surface for a selected session key. Operators cannot verify per-session timeline contents, validation integrity, or usage summary directly from SSR markers at `/ops/sessions/{session_key}`.

## Scope
In scope:
- Add deterministic SSR contracts for `/ops/sessions/{session_key}`.
- Render deterministic message timeline markers derived from selected session lineage.
- Render deterministic validation and usage summary markers for selected session.
- Preserve and regression-protect existing `/ops/sessions` list and `/ops/chat` contracts.

Out of scope:
- Session graph rendering.
- Branch/merge/reset/export action execution flows.
- New backend storage semantics.

## Acceptance Criteria
### AC-1 Route + panel contracts
Given a request to `/ops/sessions/{session_key}`,
when the shell is rendered,
then the response includes a deterministic session-detail panel marker containing the selected session key and route path.

### AC-2 Timeline row contracts
Given selected session lineage entries,
when `/ops/sessions/{session_key}` renders,
then timeline row markers are deterministic, ordered, and include role + entry-id attributes.

### AC-3 Validation-report contracts
Given selected session entries,
when `/ops/sessions/{session_key}` renders,
then validation marker attributes include `entries`, `duplicates`, `invalid_parent`, `cycles`, and `is_valid`.

### AC-4 Usage-summary contracts
Given selected session usage summary,
when `/ops/sessions/{session_key}` renders,
then usage marker attributes include `input_tokens`, `output_tokens`, `total_tokens`, and `estimated_cost_usd`.

### AC-5 Empty-session fallback contracts
Given a selected session with no timeline entries,
when `/ops/sessions/{session_key}` renders,
then deterministic empty-state timeline marker contracts are present while panel/summary markers remain present.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | valid session detail route request | render shell | includes `tau-ops-session-detail-panel` marker with selected key + route |
| C-02 | AC-2 | Integration | selected session with multiple role entries | render detail route | timeline row markers appear in lineage order with deterministic ids |
| C-03 | AC-3 | Functional | selected session entries | render detail route | validation marker attributes match `SessionValidationReport` fields |
| C-04 | AC-4 | Integration | selected session usage persisted via chat append | render detail route | usage marker attributes match `SessionUsageSummary` fields |
| C-05 | AC-5 | Functional | selected session with no non-empty entries | render detail route | timeline empty-state marker present with entry count `0` |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui functional_spec_2842 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2842' -- --test-threads=1` passes.
- Regression suites for `spec_2838` and `spec_2834` remain green.
