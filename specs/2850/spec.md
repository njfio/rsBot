# Spec: Issue #2850 - command-center recent-cycles table contracts

Status: Implemented

## Problem Statement
Tau Ops command-center renders a "Recent Cycles" table, but current SSR markup does not expose deterministic row-level attributes for verification and does not expose explicit empty-state marker semantics when timeline data is absent. Operators cannot reliably validate table values from shell contracts.

## Scope
In scope:
- Add deterministic SSR marker attributes for recent-cycles table panel and summary row on `/ops`.
- Add deterministic empty-state marker contract when timeline data is absent.
- Preserve existing timeline chart/range and command-center control contracts.

Out of scope:
- New data sources or timeline aggregation logic changes.
- Interactive table sorting/filtering.
- Dashboard API schema changes.

## Acceptance Criteria
### AC-1 Recent-cycles panel route contracts
Given a request to `/ops`,
when shell renders,
then response includes deterministic recent-cycles panel marker attributes containing route and selected timeline range.

### AC-2 Summary row field contracts
Given command-center timeline snapshot values,
when `/ops` renders,
then summary row marker attributes expose deterministic timestamp, point-count, cycle-count, and invalid-cycle-count fields.

### AC-3 Empty-state contracts
Given command-center timeline point count equals `0`,
when `/ops` renders,
then deterministic empty-state marker is present for the recent-cycles table.

### AC-4 Non-empty-state contracts
Given command-center timeline point count greater than `0`,
when `/ops` renders,
then recent-cycles empty-state marker is not present.

### AC-5 Command-center regression safety
Given existing command-center shell contracts,
when new recent-cycles contracts are added,
then timeline chart/range/control and KPI contracts remain unchanged.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | `/ops` route request with `range=6h` | render shell | `tau-ops-data-table` marker includes route + range attributes |
| C-02 | AC-2 | Functional | snapshot with non-zero timeline values | render shell | `tau-ops-timeline-summary-row` marker exposes deterministic value attributes |
| C-03 | AC-3 | Functional | snapshot with `timeline_point_count=0` | render shell | `tau-ops-timeline-empty-row` marker present |
| C-04 | AC-4 | Integration | runtime fixture with timeline points | render `/ops` through gateway | empty-state row absent and summary row attributes present |
| C-05 | AC-5 | Regression | existing command-center suites | rerun targeted specs | prior command-center contracts remain green |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui functional_spec_2850 -- --test-threads=1` passes.
- `cargo test -p tau-gateway 'spec_2850' -- --test-threads=1` passes.
- Regression suites for command-center dependencies (`spec_2806`, `spec_2814`, `spec_2826`, `spec_2818`) remain green.
