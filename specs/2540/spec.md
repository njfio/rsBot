# Spec #2540 - Story: profile-policy change detection for runtime heartbeat bridge

Status: Implemented

## Problem Statement
Runtime heartbeat supports live policy reloads, but there is no story-level contract tying active profile store mutations to that reload path.

## Acceptance Criteria
### AC-1 Profile-store change detection path exists
Given a running local runtime with heartbeat enabled, when the active profile store changes, then bridge logic observes the change and evaluates updated runtime-heartbeat policy fields.

### AC-2 Evaluation outcomes are deterministic
Given profile-store deltas, when bridge evaluation runs, then outcomes are deterministic (`applied`, `no_change`, `invalid`, `missing_profile`) and surfaced in diagnostics/logs.

## Scope
In scope:
- Profile store file change detection.
- Runtime-heartbeat policy extraction and validation.
- Deterministic bridge diagnostics.

Out of scope:
- Non-heartbeat policy fields.
- Cross-module global config reload orchestration.

## Conformance Cases
- C-01 (AC-1): task tests confirm change detection triggers evaluation.
- C-02 (AC-2): task tests verify deterministic outcome mapping and diagnostics.

## Success Metrics
- Task #2541 conformance tests pass and map to C-01/C-02.
