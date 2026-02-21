# Spec: Issue #3128 - ops channels list health contracts

Status: Reviewed

## Problem Statement
`/ops/channels` route is registered in navigation but does not render deterministic channel list health contracts required by PRD item `2103`.

## Scope
In scope:
- Add deterministic `/ops/channels` panel markers.
- Add deterministic channels summary markers (online/offline/degraded).
- Add deterministic channels row markers sourced from connector health snapshot rows.
- Add conformance and regression tests.

Out of scope:
- Channel action contracts (`login/logout/probe`).
- Channel detail lifecycle panel contracts.
- Dependency changes.

## Acceptance Criteria
### AC-1 `/ops/channels` renders deterministic channels panel contracts
Given `/ops/channels` renders,
when shell HTML is produced,
then channels panel markers are visible with deterministic channel-count contracts.

### AC-2 `/ops/channels` renders deterministic channel list health rows
Given connector health rows exist in snapshot context,
when shell HTML is produced,
then deterministic rows render channel name, mode, liveness, events, and provider failure counters.

### AC-3 gateway route `/ops/channels` renders channels contracts from runtime fixtures
Given gateway multi-channel runtime fixtures are present,
when `/ops/channels` route is rendered,
then channels panel, summary, and row markers render deterministic values.

### AC-4 non-channels routes keep hidden channels markers and regressions remain green
Given any route other than `/ops/channels` renders,
when shell HTML is produced,
then channels markers remain present and hidden, and nearby regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route `/ops/channels` | render shell | channels panel marker is visible with deterministic route/count contracts |
| C-02 | AC-2 | Functional | connector health rows in snapshot | render shell | deterministic channel rows + summary counters render |
| C-03 | AC-3 | Integration | gateway multi-channel fixture exists | render `/ops/channels` | channels panel/summary/row contracts present with deterministic values |
| C-04 | AC-4 | Regression | active route is not `/ops/channels` | render shell | channels markers remain present and hidden; nearby regressions pass |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3128 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3128 -- --test-threads=1`
- `cargo test -p tau-gateway spec_2822 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
