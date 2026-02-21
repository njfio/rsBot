# Spec: Issue #3116 - ops jobs list contracts

Status: Reviewed

## Problem Statement
`/ops/tools-jobs` currently exposes tools inventory/detail contracts but does not provide deterministic jobs-list contracts for running/completed/failed jobs (PRD item `2100`).

## Scope
In scope:
- Add deterministic jobs panel contracts on `/ops/tools-jobs`.
- Add deterministic jobs summary markers for running/completed/failed counts.
- Add deterministic jobs table row contracts (id/name/status/started/finished).
- Populate jobs rows in gateway shell snapshot.
- Add conformance and regression tests.

Out of scope:
- Job detail panel contracts (PRD `2101`).
- Job cancellation actions (PRD `2102`).
- New job execution backend/dependency changes.

## Acceptance Criteria
### AC-1 `/ops/tools-jobs` renders deterministic jobs panel contracts
Given `/ops/tools-jobs` renders,
when shell HTML is produced,
then jobs panel and summary markers expose deterministic visibility and count contracts.

### AC-2 jobs table renders deterministic running/completed/failed rows
Given jobs snapshot data is present,
when shell HTML is produced,
then jobs table row markers expose deterministic status/time contracts for running/completed/failed jobs.

### AC-3 gateway route renders jobs contracts from runtime-backed snapshot
Given gateway serves `/ops/tools-jobs`,
when the route is requested,
then jobs panel/summary/table markers render with deterministic contract values.

### AC-4 non-tools routes keep jobs contracts hidden and regressions remain green
Given any non-`/ops/tools-jobs` route renders,
when shell HTML is produced,
then jobs markers remain present and hidden, and nearby regression suites remain green.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route is `/ops/tools-jobs` | render shell | jobs panel + summary markers are visible with deterministic counts |
| C-02 | AC-2 | Functional | jobs rows include running/completed/failed entries | render shell | jobs table row markers expose deterministic status/time contracts |
| C-03 | AC-3 | Integration | gateway route `/ops/tools-jobs` | HTTP render | jobs panel/summary/table contracts are present in response body |
| C-04 | AC-4 | Regression | active route is not `/ops/tools-jobs` | render shell | jobs markers remain present/hidden; nearby regressions pass |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3116 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3116 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3112 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3112 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
