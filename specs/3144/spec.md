# Spec: Issue #3144 - /ops/config profile and policy control contracts

Status: Reviewed

## Problem Statement
The `/ops/config` route currently provides only top-level panel and endpoint markers. PRD section `6.10.1` and `6.10.2` require deterministic contract controls for profile and policy settings.

## Scope
In scope:
- Add deterministic profile controls in `/ops/config` (model, fallback models, system prompt, max turns).
- Add deterministic policy controls in `/ops/config` (tool preset, bash profile, sandbox mode, limits, heartbeat/compaction controls).
- Add UI conformance and gateway integration tests for these markers.

Out of scope:
- Persisting config changes.
- Hot-reload application logic.
- Backend endpoint additions.

## Acceptance Criteria
### AC-1 `/ops/config` renders deterministic profile control contracts
Given active route `/ops/config`,
when shell HTML is rendered,
then profile control markers are visible for model, fallback, system prompt, and max turns.

### AC-2 `/ops/config` renders deterministic policy control contracts
Given active route `/ops/config`,
when shell HTML is rendered,
then policy control markers are visible for tool/bash/sandbox/limits/heartbeat/compaction controls.

### AC-3 gateway `/ops/config` route includes profile and policy control markers
Given gateway serves `/ops/config`,
when route HTML is rendered,
then profile and policy contract markers are present.

### AC-4 non-config routes keep config control panel hidden
Given active route is not `/ops/config`,
when shell HTML is rendered,
then config panel remains hidden and markers are preserved for regression safety.

## Conformance Cases
| Case | AC | Tier | Given | When | Then |
|---|---|---|---|---|---|
| C-01 | AC-1 | Functional | active route `/ops/config` | render shell | profile control markers present/visible |
| C-02 | AC-2 | Functional | active route `/ops/config` | render shell | policy control markers present/visible |
| C-03 | AC-3 | Integration | gateway `/ops/config` request | render HTML | profile/policy markers present |
| C-04 | AC-4 | Regression | active route `/ops` | render shell | config panel hidden marker remains stable |

## Success Metrics / Signals
- `cargo test -p tau-dashboard-ui spec_3144 -- --test-threads=1`
- `cargo test -p tau-gateway spec_3144 -- --test-threads=1`
- `cargo test -p tau-dashboard-ui spec_3140 -- --test-threads=1`
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
