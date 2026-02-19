# Spec: Issue #2618 - Stage multi-process runtime architecture contracts (G1)

Status: Implemented

## Problem Statement
Tau currently executes a single agent turn loop and has branch follow-up concurrency controls, but it lacks an explicit first-class process architecture contract for `channel`, `branch`, `worker`, `compactor`, and `cortex` roles. Without a staged contract and supervisor abstraction, future migration work risks inconsistent process semantics and ad-hoc runtime branching.

## Acceptance Criteria

### AC-1 Process role contracts exist as public API in `tau-agent-core`
Given `tau-agent-core` consumers,
When they need to reason about or stage multi-process roles,
Then they can use a stable `ProcessType` contract plus per-role runtime profile defaults (system prompt, turn/context/tool boundaries).

### AC-2 Supervisor abstraction stages multi-process lifecycle management
Given a staged multi-process runtime path,
When process instances are spawned,
Then a `ProcessManager` can register/supervise lifecycle state transitions (`pending` -> `running` -> terminal) and expose deterministic snapshots for operators/tests.

### AC-3 Existing single-loop behavior remains unchanged by default
Given current agent/runtime callers,
When they do not explicitly use new process APIs,
Then existing prompt/turn/tool behavior remains unchanged and branch follow-up tests continue to pass.

### AC-4 Architecture decision and migration staging are documented
Given this architecture slice,
When future migration tasks execute,
Then an ADR documents scope, staged rollout boundary, and consequences to keep implementation aligned.

### AC-5 Scoped verification gates are green
Given this issue scope,
When formatting, linting, and targeted `tau-agent-core` tests run,
Then all checks pass.

## Scope

### In Scope
- Add `ProcessType` and per-role runtime profile contracts in `tau-agent-core`.
- Add `ProcessManager` lifecycle supervisor staging abstraction.
- Add focused tests for process contract defaults and lifecycle supervision behavior.
- Add ADR describing staged multi-process architecture migration boundaries.

### Out of Scope
- Full conversion of agent turn loop into multi-process execution runtime.
- Gateway/admin endpoints for cortex process control.
- Provider/model routing changes beyond existing per-process model config work.

## Conformance Cases
- C-01 (unit): `ProcessType` and profile defaults are deterministic and role-specific.
- C-02 (unit): `ProcessManager` starts/supervises process lifecycle transitions and exposes stable snapshots.
- C-03 (regression): existing branch/single-loop tests remain green without opting into process manager APIs.
- C-04 (docs): ADR captures staged migration decision and consequences.
- C-05 (verify): `cargo fmt --check`, `cargo clippy -p tau-agent-core -- -D warnings`, and targeted `tau-agent-core` tests pass.

## Success Metrics / Observable Signals
- Process role semantics are codified as compile-time contracts rather than ad-hoc conventions.
- Supervisor state is inspectable and deterministic for future orchestration integration.
- No default runtime behavior regressions are introduced.
