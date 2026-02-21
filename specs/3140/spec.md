# Spec: Issue #3140 - ops route panel contracts for config/training/safety/diagnostics

Status: Reviewed

## Problem Statement
The Tau Ops shell navigation exposes `/ops/config`, `/ops/training`, `/ops/safety`, and `/ops/diagnostics`, but the rendered shell does not provide dedicated route panels and deterministic route-level contracts for these views.

## Scope
In scope:
- Add dedicated route panels for `/ops/config`, `/ops/training`, `/ops/safety`, `/ops/diagnostics`.
- Add deterministic panel visibility contracts and endpoint-template markers per route.
- Add conformance tests in `tau-dashboard-ui` and gateway integration tests in `tau-gateway`.

Out of scope:
- Live mutation handlers for config/safety/training/audit operations.
- New backend API endpoints.
- Dependency changes.

## Acceptance Criteria
### AC-1 `/ops/config` renders deterministic configuration panel contracts
Given active route `/ops/config`,
when shell HTML is rendered,
then configuration panel markers are visible and include deterministic config endpoint templates.

### AC-2 `/ops/training` renders deterministic training panel contracts
Given active route `/ops/training`,
when shell HTML is rendered,
then training panel markers are visible and include deterministic status/rollout endpoint templates.

### AC-3 `/ops/safety` and `/ops/diagnostics` render deterministic panel contracts
Given active routes `/ops/safety` and `/ops/diagnostics`,
when shell HTML is rendered,
then each route has a dedicated visible panel with deterministic policy/audit endpoint templates.

### AC-4 gateway `/ops/*` route rendering honors panel visibility contracts
Given gateway serves shell routes for config/training/safety/diagnostics,
when each route is requested,
then the corresponding panel is visible and route markers are present.

### AC-5 non-target routes keep new panels hidden
Given active route is not config/training/safety/diagnostics,
when shell HTML is rendered,
then new panels are present but hidden and existing route behavior stays unchanged.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route `/ops/config` | render shell | config panel is visible with deterministic endpoint template markers |
| C-02 | AC-2 | Functional | active route `/ops/training` | render shell | training panel is visible with deterministic endpoint template markers |
| C-03 | AC-3 | Functional | active routes `/ops/safety` and `/ops/diagnostics` | render shell | each panel is visible with deterministic endpoint template markers |
| C-04 | AC-4 | Integration | gateway serves `/ops/config`, `/ops/training`, `/ops/safety`, `/ops/diagnostics` | HTTP render | matching panel route markers are visible in HTML |
| C-05 | AC-5 | Regression | active route `/ops` | render shell | config/training/safety/diagnostics panels remain hidden |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3140 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3140 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3132 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
