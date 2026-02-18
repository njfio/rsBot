# Spec #2465 - add heartbeat scheduler policy reload path + conformance tests

Status: Implemented

## Problem Statement
`G16` phase-1 requires runtime hot-reload behavior for heartbeat scheduler policy so operators can tune cadence without process restart.

## Acceptance Criteria
### AC-1 Policy interval updates apply without restart
Given a running heartbeat scheduler and a sidecar policy file for the scheduler state path, when `interval_ms` changes to a valid value, then later snapshots use the updated interval and the process remains running.

### AC-2 Unchanged policy does not mutate scheduler behavior
Given a running heartbeat scheduler and no policy file changes, when heartbeat cycles execute, then interval remains at the last effective value and no reload reason code is emitted.

### AC-3 Invalid policy is fail-closed and observable
Given a running heartbeat scheduler and malformed policy content, when reload evaluation occurs, then scheduler keeps last-known-good interval, continues running, and emits a deterministic reload-failure reason code/diagnostic.

## Scope
In scope:
- `tau-runtime` heartbeat scheduler policy hot-reload path.
- Phase-1 policy field: `interval_ms`.
- Conformance/regression tests in `tau-runtime`.

Out of scope:
- Full config/profile hot-reload across Tau modules.
- New dependency adoption for file watching.
- Prompt/template reload behavior.

## Conformance Cases
- C-01 (AC-1, integration): `integration_spec_2465_c01_runtime_heartbeat_hot_reload_applies_interval_updates`
- C-02 (AC-2, regression): `regression_spec_2465_c02_runtime_heartbeat_without_policy_change_keeps_interval_stable`
- C-03 (AC-3, regression): `regression_spec_2465_c03_runtime_heartbeat_invalid_hot_reload_policy_preserves_last_good_interval`

## Success Metrics
- All C-01..C-03 tests pass.
- No runtime panic/crash when policy file is malformed.
- Snapshot `interval_ms` reflects effective active interval over time.
