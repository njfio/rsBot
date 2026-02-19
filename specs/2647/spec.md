# Spec: Issue #2647 - External coding-agent subprocess worker support in bridge runtime (G21 phase 3)

Status: Reviewed

## Problem Statement
Tau's G21 bridge currently provides session lifecycle, follow-up queueing, and SSE-ready event replay, but does not execute a real external coding-agent subprocess per worker session. This leaves the final G21 parity item incomplete (`Add external coding agent subprocess support to worker system`) and prevents live worker process supervision through the existing bridge/session APIs.

## Acceptance Criteria

### AC-1 Bridge can launch a configured subprocess worker for new sessions
Given external coding-agent bridge config includes a subprocess command,
When a new workspace session is opened,
Then the bridge launches one subprocess for that session and records deterministic running lifecycle state.

### AC-2 Session reuse and follow-up routing preserve single-process behavior
Given a running session already exists for a workspace,
When open/reuse and follow-up operations execute,
Then the same session/subprocess is reused and follow-up messages are forwarded to subprocess stdin while queue semantics remain deterministic.

### AC-3 Subprocess output is available through ordered bridge progress events
Given a subprocess writes stdout/stderr output,
When bridge events are polled,
Then output lines are emitted as ordered progress events suitable for existing SSE replay consumers.

### AC-4 Close/reap lifecycle paths safely terminate subprocess workers
Given a running subprocess-backed session,
When session close or inactivity reap occurs,
Then subprocess execution is terminated safely and terminal lifecycle state is reflected in bridge snapshots.

### AC-5 Non-subprocess mode preserves existing behavior
Given subprocess config is unset,
When bridge lifecycle/progress/follow-up APIs execute,
Then legacy behavior remains valid and existing gateway tests keep passing without API shape changes.

### AC-6 Scoped verification gates pass
Given this scope,
When formatting, linting, and targeted tests run,
Then `cargo fmt --check`, `cargo clippy -p tau-runtime -- -D warnings`, `cargo clippy -p tau-gateway -- -D warnings`, and targeted runtime/gateway tests pass.

## Scope

### In Scope
- Extend `tau-runtime::external_coding_agent_bridge_runtime` with optional subprocess launch + supervision support.
- Capture stdout/stderr process output into bridge event stream.
- Forward follow-up messages into subprocess stdin while keeping queued follow-up APIs intact.
- Ensure lifecycle close/reap paths terminate subprocess processes.
- Keep gateway HTTP+SSE handlers compatible with current request/response schema.

### Out of Scope
- Replacing gateway endpoint contracts introduced in #2638.
- Distributed/multi-host subprocess pool orchestration.
- New external coding-agent protocol beyond local subprocess stdin/stdout/stderr integration.

## Conformance Cases
- C-01 (unit): opening a subprocess-configured session launches subprocess and records running snapshot.
- C-02 (functional): opening same workspace reuses running session and does not spawn duplicate subprocess.
- C-03 (functional): follow-up message is queued and forwarded to subprocess stdin.
- C-04 (conformance): subprocess stdout/stderr lines appear in ordered bridge event polling output.
- C-05 (regression): close session terminates subprocess and removes active session mapping.
- C-06 (regression): inactivity reaper terminates stale subprocess sessions and reports timed-out state.
- C-07 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Remaining G21 subprocess checkbox is completed with test evidence.
- Bridge session events reflect real subprocess output without endpoint contract drift.
- Session lifecycle cleanup prevents subprocess leakage on close/reap.
