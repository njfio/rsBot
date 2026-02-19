# Spec #2544 - Task: stabilize runtime heartbeat hot-reload convergence for spec_2465/spec_2487

Status: Accepted

## Problem Statement
Workspace verification is blocked by reproducible failures in `tau-runtime` heartbeat hot-reload tests where interval convergence does not occur before timeout, even though policy files are written. This indicates watcher-event race/miss behavior that requires a deterministic fallback.

## Acceptance Criteria
### AC-1 Hot-reload converges when file watcher events are missed
Given a running heartbeat scheduler with hot-reload watcher enabled, when policy file content changes but no watcher event is delivered, then runtime still detects the change and applies the new interval deterministically.

### AC-2 Pending-reload guard remains fail-closed when watcher path is unavailable
Given evaluation state with `pending_policy_reload=false` and no watcher/polling channel, when a policy file exists, then evaluation does not apply policy changes.

### AC-3 Invalid policy payload remains fail-closed
Given invalid policy payload content after a valid interval is active, when evaluation runs, then interval remains last-known-good and deterministic invalid diagnostics/reason-codes are emitted.

### AC-4 Existing G16 heartbeat conformance tests pass reliably
Given the hot-reload policy suite (`spec_2465/spec_2487`), when tests run serially on CI-like settings, then convergence tests pass without timeout failures.

### AC-5 Pending-reload short-circuit does not emit poll fallback diagnostics
Given hot-reload state with `pending_policy_reload=true` and watcher context present, when evaluation executes, then the pending reload path applies policy without poll-fallback side effects.

## Scope
In scope:
- `crates/tau-runtime/src/heartbeat_runtime.rs` hot-reload state/evaluation logic.
- Deterministic fallback detection for policy file changes.
- Regression/conformance tests for watcher-miss fallback behavior.

Out of scope:
- New hot-reload feature surface outside heartbeat runtime.
- Scheduler architecture redesign.

## Conformance Cases
- C-01 (AC-1): `integration_spec_2544_c01_runtime_heartbeat_hot_reload_applies_interval_when_watcher_event_is_missed`
- C-02 (AC-2): `regression_spec_2544_c02_hot_reload_requires_pending_signal_without_watcher_or_poll_context`
- C-03 (AC-3): `regression_spec_2544_c03_invalid_policy_after_valid_update_preserves_last_interval`
- C-04 (AC-4): `integration_spec_2465_c01_runtime_heartbeat_hot_reload_applies_interval_updates`, `integration_spec_2487_c01_runtime_heartbeat_profile_toml_hot_reload_applies_interval_update`, `functional_spec_2487_c04_runtime_heartbeat_hot_reload_uses_arc_swap_active_config`
- C-05 (AC-5): `regression_spec_2544_c05_pending_reload_short_circuits_poll_fallback_with_watcher_context`

## Success Metrics
- C-01..C-04 pass.
- C-05 passes.
- `cargo fmt --check`, `cargo clippy -- -D warnings`, scoped `tau-runtime` tests, mutation in diff, live validation, and workspace `cargo test -j 1` pass.
