# Spec: Issue #3148 - /ops/training status and control contracts

Status: Reviewed

## Problem Statement
The `/ops/training` route currently exposes only endpoint template markers. PRD section `6.11` requires deterministic route contracts for status, rollouts, optimizer report, and control actions.

## Scope
In scope:
- Add deterministic training status markers on `/ops/training`.
- Add deterministic rollout history and optimizer report markers.
- Add deterministic pause/reset/export action markers.
- Add UI conformance + gateway integration tests.

Out of scope:
- Live training orchestration behavior changes.
- New backend endpoints.
- Persisting training configuration mutations.

## Acceptance Criteria
### AC-1 `/ops/training` renders deterministic training status markers
Given active route `/ops/training`,
when shell HTML is rendered,
then status markers include running state, gate, store path, update interval, max rollouts, and failure streak.

### AC-2 `/ops/training` renders deterministic rollout and optimizer markers
Given active route `/ops/training`,
when shell HTML is rendered,
then rollout history and optimizer summary markers are present with deterministic values.

### AC-3 `/ops/training` renders deterministic training control action markers
Given active route `/ops/training`,
when shell HTML is rendered,
then pause/reset/export action markers and endpoint templates are present.

### AC-4 gateway `/ops/training` route includes training contract markers
Given gateway serves `/ops/training`,
when route HTML is rendered,
then status/rollout/optimizer/action markers are present.

### AC-5 non-training routes keep training panel hidden
Given active route is not `/ops/training`,
when shell HTML is rendered,
then training panel stays hidden and route regressions remain stable.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route `/ops/training` | render shell | status marker set present |
| C-02 | AC-2 | Functional | active route `/ops/training` | render shell | rollout and optimizer markers present |
| C-03 | AC-3 | Functional | active route `/ops/training` | render shell | training action markers/endpoints present |
| C-04 | AC-4 | Integration | gateway `/ops/training` request | render HTML | training route contract markers present |
| C-05 | AC-5 | Regression | active route `/ops` | render shell | training panel hidden marker stable |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3148 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3148 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3144 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
