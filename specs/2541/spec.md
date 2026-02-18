# Spec #2541 - Task: profile-driven runtime heartbeat hot-reload bridge

Status: Implemented

## Problem Statement
`G16` requires live configuration reload behavior, but Tau currently updates runtime heartbeat from sidecar policy changes only. There is no bridge from active profile store updates to heartbeat reload payloads.

Tau uses `.tau/profiles.json` as its profile source-of-truth (JSON equivalent of the comparison doc's profile TOML contract). This task closes the bounded bridge for runtime heartbeat policy fields.

## Acceptance Criteria
### AC-1 Profile policy updates are projected into heartbeat policy payload
Given a valid profile store update for the active profile runtime heartbeat interval, when the bridge evaluates changes, then it writes an updated `<state-path>.policy.toml` payload with the new interval.

### AC-2 No-op updates are stable and do not churn policy artifacts
Given profile updates that do not change the effective runtime heartbeat interval, when evaluation runs, then no new policy payload is written and deterministic `no_change` diagnostics are emitted.

### AC-3 Invalid profile payloads fail closed
Given malformed/unreadable profile store content or invalid heartbeat interval values, when evaluation runs, then the bridge preserves last-known-good interval behavior and emits deterministic `invalid` diagnostics.

### AC-4 Bridge lifecycle starts and shuts down cleanly
Given runtime heartbeat bridge startup/shutdown, when heartbeat is enabled, then the bridge task starts with deterministic initial evaluation and shuts down cleanly without panic or leaked task.

## Scope
In scope:
- Profile store change detection for `.tau/profiles.json`.
- Runtime-heartbeat interval bridge into `<state-path>.policy.toml`.
- Deterministic diagnostics for apply/no-change/invalid/missing-profile.
- Local runtime lifecycle wiring and tests.

Out of scope:
- Hot reload for non-heartbeat policy fields.
- Dynamic heartbeat enable/disable and state-path migration while running.
- Cross-process profile synchronization.

## Conformance Cases
- C-01 (AC-1, integration): `integration_spec_2541_c01_profile_policy_bridge_applies_updated_interval_policy`
- C-02 (AC-2, regression): `regression_spec_2541_c02_profile_policy_bridge_no_change_does_not_rewrite_policy_file`
- C-03 (AC-3, regression): `regression_spec_2541_c03_profile_policy_bridge_invalid_profile_store_preserves_last_interval`
- C-04 (AC-4, integration): `integration_spec_2541_c04_profile_policy_bridge_start_and_shutdown_is_clean`

## Success Metrics
- C-01..C-04 all pass.
- `cargo fmt --check`, `cargo clippy -- -D warnings`, scoped tests, full `cargo test`, and live demo validation pass.
