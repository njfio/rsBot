# Spec: Issue #2638 - Gateway external coding-agent APIs and SSE stream (G21 phase 2)

Status: Implemented

## Problem Statement
Tau has runtime bridge contracts for external coding-agent sessions (#2619), but no gateway-facing API layer for opening/reusing sessions, routing follow-ups, replaying progress via SSE, and exposing bridge runtime state in operator status views. Without this integration, the bridge remains inaccessible to live operators and tooling.

## Acceptance Criteria

### AC-1 Gateway exposes authenticated external coding-agent session lifecycle APIs
Given gateway auth succeeds,
When an operator opens/reuses a workspace session and later inspects/closes it,
Then the API returns deterministic bridge snapshots and lifecycle transitions.

### AC-2 Gateway exposes follow-up routing APIs for interactive continuation
Given an active external coding-agent session,
When operators enqueue follow-up messages and drain queued follow-ups,
Then follow-up queue counts and payload order are preserved.

### AC-3 Gateway exposes SSE progress replay over bridge events
Given progress/follow-up events exist for a session,
When an operator calls the stream endpoint with optional replay cursor,
Then SSE emits ordered event payloads and terminates with a done frame.

### AC-4 Gateway status includes external coding-agent endpoint metadata and runtime snapshot
Given gateway status is requested,
When external coding-agent integration is enabled,
Then response payload includes endpoint paths and current bridge runtime counters/config.

### AC-5 Timeout cleanup is exposed through gateway API behavior
Given session inactivity exceeds configured timeout,
When cleanup executes via gateway API,
Then stale sessions are reaped and reported as timed out.

### AC-6 Scoped verification gates pass
Given this task scope,
When formatting/linting/tests run,
Then `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, and `cargo test -p tau-gateway` pass.

## Scope

### In Scope
- Add external coding-agent endpoints to gateway router with auth/rate-limit enforcement.
- Wire `ExternalCodingAgentBridge` into gateway runtime state.
- Add SSE replay endpoint and related request/response structures.
- Add gateway status payload section for external coding-agent integration.
- Add tests covering lifecycle, follow-up routing, SSE replay, and timeout reaping.

### Out of Scope
- Spawning real external coding-agent subprocesses.
- Distributed multi-host bridge pools.
- UI redesign beyond exposing endpoint metadata already consumed by existing webchat tooling.

## Conformance Cases
- C-01 (functional): open/reuse session, inspect snapshot, close session via gateway endpoints.
- C-02 (functional): enqueue + drain follow-ups preserves FIFO order and queue counts.
- C-03 (conformance): SSE stream returns ordered events and done frame, replay cursor respected.
- C-04 (conformance): gateway status includes external coding-agent endpoint/runtime section.
- C-05 (regression): timeout reap endpoint removes stale sessions and reports timed-out status.
- C-06 (verify): fmt/clippy/tau-gateway test suite passes.

## Success Metrics / Observable Signals
- External coding-agent bridge is operable from authenticated gateway APIs.
- Operators can consume deterministic SSE progress replay from bridge events.
- Timeout cleanup is observable and test-backed.
