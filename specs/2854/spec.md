# Spec: Issue #2854 - command-center route visibility contracts

Status: Implemented

## Problem Statement
Tau Ops renders command-center content with deterministic contract markers, but command-center panel visibility is not route-gated. On non-command-center routes, operators cannot deterministically validate that command-center surface is intentionally hidden.

## Scope
In scope:
- Add deterministic route visibility marker contracts for command-center panel.
- Ensure `/ops` exposes command-center panel visible state.
- Ensure non-command-center ops routes expose command-center panel hidden state.

Out of scope:
- Command-center data aggregation changes.
- Navigation menu changes.
- API or transport schema changes.

## Acceptance Criteria
### AC-1 `/ops` command-center visibility contract
Given a request to `/ops`,
when shell renders,
then command-center panel marker includes deterministic `aria-hidden="false"` with command-center route metadata.

### AC-2 non-command-center hidden contract (`/ops/chat`)
Given a request to `/ops/chat`,
when shell renders,
then command-center panel marker includes deterministic `aria-hidden="true"`.

### AC-3 non-command-center hidden contract (`/ops/sessions`)
Given a request to `/ops/sessions`,
when shell renders,
then command-center panel marker includes deterministic `aria-hidden="true"`.

### AC-4 Existing command-center markers preserved
Given `/ops` command-center route,
when visibility contract is added,
then existing command-center chart/control/kpi markers remain present and unchanged.

### AC-5 Route panel regression safety
Given existing chat/sessions visibility contracts,
when command-center visibility contract is added,
then chat/sessions panel visibility markers remain unchanged.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops` route | render shell | `tau-ops-command-center` includes `data-route="/ops"` and `aria-hidden="false"` |
| C-02 | AC-2 | Functional | `/ops/chat` route | render shell | `tau-ops-command-center` includes `aria-hidden="true"` |
| C-03 | AC-3 | Integration | `/ops/sessions` route through gateway | render shell | `tau-ops-command-center` includes `aria-hidden="true"` |
| C-04 | AC-4 | Regression | `/ops` with runtime fixtures | render shell | existing command-center markers still present |
| C-05 | AC-5 | Regression | existing chat/sessions suites | rerun targeted specs | existing chat/sessions visibility contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui functional_spec_2854 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2854' -- --test-threads=1` passes.
- Regression suites for dependencies (`spec_2806`, `spec_2830`, `spec_2838`, `spec_2842`) remain green.
