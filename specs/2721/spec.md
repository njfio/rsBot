# Spec: Issue #2721 - Integrate ProcessManager/runtime profiles into live branch-worker execution

Status: Implemented

## Problem Statement
`tau-agent-core` already defines `ProcessType`, role profiles, and a `ProcessManager`, but the live branch follow-up path executes directly without supervisor lifecycle registration. This leaves the G1 checklist partially open because channel delegation lineage and worker-role constraints are not enforced by runtime contracts.

## Acceptance Criteria

### AC-1 Channel delegation registers supervised branch + worker process lineage
Given a channel process executes a `branch` tool call,
When branch follow-up is triggered,
Then the runtime registers a supervised branch process and supervised worker child process with deterministic parent-child lineage.

### AC-2 Worker follow-up executes in isolated tokio task with Worker profile limits
Given branch follow-up execution,
When the delegated worker process runs,
Then it executes in an isolated tokio task under `ProcessType::Worker` profile (`max_turns=25`, worker system prompt/context window, memory-only tool allowlist).

### AC-3 Parent receives structured delegation metadata from process execution
Given successful delegated execution,
When branch tool result is returned to parent turn,
Then the payload includes deterministic process metadata (`channel`, `branch`, `worker` ids/types and lifecycle states) alongside branch conclusion.

### AC-4 Supervisor tracks terminal states for success and failure paths
Given successful and failed delegated runs,
When process tasks complete,
Then `ProcessManager` snapshots transition to terminal states (`Completed` or `Failed`) with deterministic diagnostics.

### AC-5 Existing branch guardrails remain intact
Given prior branch constraints and malformed prompt behavior,
When this integration is introduced,
Then concurrency limits, missing-prompt fail-closed handling, and memory-only branch tool enforcement continue to pass existing regressions.

### AC-6 Scoped verification gates pass
Given this implementation slice,
When scoped checks run,
Then `cargo fmt --check`, `cargo clippy -p tau-agent-core -- -D warnings`, and targeted `spec_2721` + `spec_2602` tests pass.

## Scope

### In Scope
- Wire `ProcessManager` into branch follow-up execution path.
- Introduce process context/runtime profile application for delegated branch/worker runs.
- Add process metadata to branch tool result payload.
- Add conformance/regression tests for lineage, worker profile enforcement, and terminal states.
- Update G1 checklist lines completed by this slice.

### Out of Scope
- Full global migration of all prompt turns to `ProcessManager`.
- New transport adapters or new dependency families.
- Dashboard UI work for process visualization.

## Conformance Cases
- C-01 (integration): successful branch follow-up registers channel->branch->worker lineage in process snapshots.
- C-02 (functional): worker execution enforces Worker profile limits (`max_turns=25`) and memory-only tools.
- C-03 (integration): branch tool result payload contains delegation metadata for process ids/types/states.
- C-04 (regression): failing delegated execution marks worker/branch terminal states as failed with diagnostics.
- C-05 (regression): existing `spec_2602` branch limit/malformed-prompt behavior remains green.
- C-06 (verify): scoped fmt/clippy/tests pass.

## Success Metrics / Observable Signals
- Branch follow-up lifecycle can be inspected deterministically through process snapshots.
- Parent turn responses include auditable delegation metadata.
- Existing branch feature behavior remains stable while adding supervised multi-process execution.
