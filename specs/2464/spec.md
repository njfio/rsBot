# Spec #2464 - runtime heartbeat policy hot-reload without restart

Status: Implemented

## Problem Statement
Runtime heartbeat scheduler policy is currently fixed at startup. Operators cannot tune heartbeat cadence without restarting the process.

## Acceptance Criteria
### AC-1 Reload behavior is available for runtime heartbeat policy
Given an active runtime heartbeat scheduler, when policy file changes, then scheduler applies supported policy changes without restart.

### AC-2 Reload behavior is deterministic and observable
Given policy changes or invalid policy payloads, when reload logic executes, then snapshots/reason codes/diagnostics describe what happened and runtime stays healthy.

## Scope
In scope:
- Runtime heartbeat policy reload behavior.
- Deterministic diagnostics and reason-code signaling.

Out of scope:
- Non-heartbeat runtime config surfaces.
- Full profile command/store synchronization semantics.

## Conformance Cases
- C-01 (AC-1, functional): policy interval change applies on subsequent heartbeat cycles.
- C-02 (AC-2, regression): invalid policy keeps previous interval and records reload failure reason code.

## Success Metrics
- Conformance cases mapped in #2465 pass in `tau-runtime` tests.
