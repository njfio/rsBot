# Spec: Issue #3132 - ops channels action contracts

Status: Reviewed

## Problem Statement
`/ops/channels` now renders deterministic channel list health contracts (PRD `2103`) but does not expose deterministic per-channel action contracts required by PRD item `2104`.

## Scope
In scope:
- Add deterministic `login` / `logout` / `probe` action markers for each channel row.
- Add deterministic action enabled/disabled contract states from channel liveness.
- Add conformance and regression tests.

Out of scope:
- Runtime execution of channel actions.
- Channel detail lifecycle panel contracts.
- Dependency changes.

## Acceptance Criteria
### AC-1 `/ops/channels` renders deterministic action markers for each channel row
Given `/ops/channels` renders with channels rows,
when shell HTML is produced,
then each row contains deterministic `login`, `logout`, and `probe` action markers.

### AC-2 action enabled/disabled state contracts are deterministic by liveness
Given channel liveness is known (`open`/`online` vs `offline`/`unknown`),
when shell HTML is produced,
then login/logout/probe action enabled contracts are deterministic.

### AC-3 gateway route `/ops/channels` renders deterministic channel action contracts
Given gateway renders `/ops/channels` from runtime fixture data,
when shell HTML is produced,
then deterministic action markers and enabled contracts are present for rendered channels.

### AC-4 non-channels routes keep hidden channels/action markers and regressions remain green
Given any route other than `/ops/channels` renders,
when shell HTML is produced,
then channels markers remain present and hidden and regressions remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route `/ops/channels` | render shell | each channel row includes deterministic login/logout/probe markers |
| C-02 | AC-2 | Functional | channels rows with mixed liveness | render shell | action enabled/disabled contracts match deterministic mapping |
| C-03 | AC-3 | Integration | gateway route `/ops/channels` with runtime fixture | HTTP render | deterministic action markers + action enabled contracts present |
| C-04 | AC-4 | Regression | active route is not `/ops/channels` | render shell | channels panel markers remain present/hidden; nearby regressions pass |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3132 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3132 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3128 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
