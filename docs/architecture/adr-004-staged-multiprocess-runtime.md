# ADR-004: Staged Multi-Process Runtime Contracts in `tau-agent-core`

## Context

Tau currently runs a single-loop agent runtime with branch follow-up support, but it does not expose first-class process role contracts for the multi-process architecture gap (G1: channel, branch, worker, compactor, cortex). Full migration is large and cross-cutting, so we need a staged boundary that allows incremental implementation without destabilizing existing behavior.

## Decision

Introduce additive, staged contracts in `tau-agent-core`:

1. `ProcessType` enum with five role variants (`Channel`, `Branch`, `Worker`, `Compactor`, `Cortex`).
2. `ProcessRuntimeProfile` defaults per role (system prompt, turn/context limits, tool allowlist intent).
3. `ProcessSpawnSpec` and `ProcessSnapshot` payloads for supervised lifecycle metadata.
4. `ProcessManager` lifecycle supervisor API for spawn + snapshot state tracking (`running`, `completed`, `failed`, `cancelled`).

This stage does **not** replace the existing turn loop. Existing runtime callers continue to use current behavior unless they explicitly invoke these new APIs.

## Consequences

### Positive
- Encodes process-role semantics as compile-time contracts rather than ad-hoc conventions.
- Enables follow-up tasks to wire real multi-process orchestration with consistent metadata.
- Preserves current single-loop runtime stability while adding migration scaffolding.

### Negative
- Adds new public API surface that must be maintained as follow-up phases evolve.
- `ProcessManager` is currently a lifecycle supervisor scaffold, not a full execution/orchestration engine.

### Follow-up
- Wire process profiles into runtime execution path and gateway/admin surfaces in subsequent tasks.
- Add cross-process scheduling, backpressure, and isolation policies once execution migration begins.
