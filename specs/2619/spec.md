# Spec: Issue #2619 - External coding-agent bridge protocol staging (G21)

Status: Implemented

## Problem Statement
Tau does not have a first-class runtime bridge for delegated external coding-agent sessions with lifecycle tracking, streaming progress replay, follow-up context injection, and inactivity cleanup. Without this bridge contract, delegated coding workflows risk ad-hoc process control, weak timeout guarantees, and non-replayable progress state.

## Acceptance Criteria

### AC-1 Session lifecycle contract exists for delegated external coding-agent runs
Given delegated coding workflows,
When a workspace opens an external coding-agent session,
Then the runtime can create/reuse a pooled session with explicit lifecycle state and deterministic snapshot metadata.

### AC-2 Progress streaming and follow-up interaction contract is implemented
Given an active external coding-agent session,
When runtime components append progress updates and follow-up prompts,
Then consumers can poll ordered event streams for SSE forwarding and inspect queued follow-ups.

### AC-3 Inactivity timeout cleanup is enforced with deterministic reaping behavior
Given no activity beyond configured inactivity window,
When cleanup runs,
Then stale sessions are marked timed-out and removed from active pool.

### AC-4 Protocol boundary is documented
Given this bridge protocol stage,
When follow-up integration work lands,
Then an ADR captures protocol framing, lifecycle semantics, and cleanup boundary decisions.

### AC-5 Scoped verification gates are green
Given this issue scope,
When formatting, linting, and targeted runtime tests run,
Then all checks pass.

## Scope

### In Scope
- Add runtime bridge/session-pool contract module for external coding-agent lifecycle.
- Add progress event/follow-up queue APIs suitable for SSE adapter wiring.
- Add inactivity timeout reaper behavior.
- Add ADR describing protocol stage and constraints.
- Add focused unit/functional/regression tests for these contracts.

### Out of Scope
- Spawning real external subprocesses.
- Full HTTP+SSE gateway endpoints for external coding-agent traffic.
- Cross-host distributed pool orchestration.

## Conformance Cases
- C-01 (unit): session open/reuse lifecycle snapshots are deterministic.
- C-01b (unit): reopening a workspace after terminal session state creates a fresh running session.
- C-02 (functional): progress events and follow-up events are emitted in ordered stream form.
- C-03 (regression): stale sessions are reaped after inactivity timeout.
- C-04 (docs): ADR documents protocol boundary decisions and consequences.
- C-05 (verify): `cargo fmt --check`, `cargo clippy -p tau-runtime -- -D warnings`, and targeted runtime tests pass.

## Success Metrics / Observable Signals
- Delegated coding sessions have inspectable lifecycle state rather than implicit process-only control.
- Progress/follow-up events are replayable and stable for SSE adapters.
- Timeout cleanup behavior is deterministic and test-backed.
