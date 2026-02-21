# Spec: Issue #3120 - ops job detail output contracts

Status: Implemented

## Problem Statement
`/ops/tools-jobs` now lists jobs (PRD `2100`) but does not expose deterministic job-detail output contracts required by PRD `2101`.

## Scope
In scope:
- Add deterministic job-detail panel contracts on `/ops/tools-jobs`.
- Add deterministic output contracts for selected job (status, duration, stdout, stderr).
- Add query selection contract for requested job id.
- Populate gateway snapshot for selected job detail output.
- Add conformance and regression tests.

Out of scope:
- Job cancellation action contracts (PRD `2102`).
- Real backend job orchestration changes.
- Dependency changes.

## Acceptance Criteria
### AC-1 `/ops/tools-jobs` renders deterministic job-detail panel contracts
Given `/ops/tools-jobs` renders with jobs rows,
when shell HTML is produced,
then job-detail panel markers expose selected job id and visibility contracts.

### AC-2 selected job detail renders deterministic output contracts
Given selected job output data exists,
when shell HTML is produced,
then status/duration/stdout/stderr markers render deterministic values.

### AC-3 gateway route resolves selected job detail contracts
Given `/ops/tools-jobs` request optionally provides a selected job id,
when gateway renders the route,
then selected job detail contracts render deterministic values for matched jobs.

### AC-4 non-tools routes keep hidden job-detail markers and regressions remain green
Given any non-`/ops/tools-jobs` route renders,
when shell HTML is produced,
then job-detail markers remain present and hidden, and nearby regressions remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route `/ops/tools-jobs` with jobs rows | render shell | detail panel marker shows selected job id and visible state |
| C-02 | AC-2 | Functional | selected job output fields provided | render shell | status/duration/stdout/stderr markers render deterministic values |
| C-03 | AC-3 | Integration | gateway route `/ops/tools-jobs?job=<id>` | HTTP render | selected job detail markers/output contracts match requested job |
| C-04 | AC-4 | Regression | active route is not `/ops/tools-jobs` | render shell | detail markers present and hidden; nearby regression suites pass |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3120 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3120 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3116 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3116 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
