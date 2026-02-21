# Spec: Issue #3124 - ops job cancel action contracts

Status: Reviewed

## Problem Statement
`/ops/tools-jobs` now exposes deterministic jobs list and job detail output contracts (PRD `2100` and `2101`) but does not expose deterministic cancel action contracts required by PRD item `2102`.

## Scope
In scope:
- Add deterministic cancel action markers for jobs rows.
- Add deterministic cancel contract panel for requested cancel actions.
- Add gateway query contract for requested cancel job id.
- Apply deterministic cancel outcomes to fixture jobs rows.
- Add conformance and regression tests.

Out of scope:
- Real runtime cancellation wiring to `/gateway/jobs/{job_id}/cancel`.
- Job queue persistence or orchestration changes.
- Dependency changes.

## Acceptance Criteria
### AC-1 `/ops/tools-jobs` renders deterministic cancel action markers
Given `/ops/tools-jobs` renders with jobs rows,
when shell HTML is produced,
then each jobs row renders deterministic cancel action markers including enabled/disabled contract state.

### AC-2 selected jobs expose deterministic cancel request contract panel
Given `/ops/tools-jobs` receives a cancel request contract for a job id,
when shell HTML is produced,
then cancel contract panel markers render requested job id and deterministic cancel status outcome.

### AC-3 gateway route resolves deterministic cancel outcomes for requested jobs
Given gateway renders `/ops/tools-jobs?cancel_job=<id>`,
when requested job id is valid and cancellable or not,
then deterministic cancel outcome contracts are rendered and jobs rows reflect deterministic status transitions.

### AC-4 non-tools routes keep hidden cancel markers and regressions remain green
Given any non-`/ops/tools-jobs` route renders,
when shell HTML is produced,
then cancel markers remain present and hidden, and nearby regressions remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route `/ops/tools-jobs` with running/completed jobs | render shell | jobs rows include deterministic cancel action markers with per-row enabled/disabled contracts |
| C-02 | AC-2 | Functional | cancel request state provided in snapshot | render shell | cancel panel markers show requested job id and deterministic cancel status |
| C-03 | AC-3 | Integration | gateway route `/ops/tools-jobs?cancel_job=job-001` | HTTP render | requested running job transitions deterministically to `cancelled`; cancel panel contracts show `cancelled` |
| C-04 | AC-4 | Regression | active route is not `/ops/tools-jobs` | render shell | cancel contract markers remain present/hidden; nearby regression suites pass |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3124 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3124 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3120 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3120 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
