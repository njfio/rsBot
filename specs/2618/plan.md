# Plan: Issue #2618 - Stage multi-process runtime architecture contracts (G1)

## Approach
1. Add RED tests for process role defaults and supervisor lifecycle behavior.
2. Implement `ProcessType`, process runtime profiles, and a lightweight `ProcessManager` supervisor in `tau-agent-core`.
3. Add ADR documenting staged migration boundary and rationale.
4. Run scoped verification gates and map AC/C evidence in PR.

## Affected Modules
- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-agent-core/src/process_types.rs` (new)
- `crates/tau-agent-core/src/tests/process_architecture.rs` (new)
- `docs/architecture/adr-004-staged-multiprocess-runtime.md` (new)
- `specs/2618/spec.md`
- `specs/2618/plan.md`
- `specs/2618/tasks.md`

## Risks / Mitigations
- Risk: introducing public API that is too rigid for later phases.
  - Mitigation: stage minimal contracts (type/profile/supervisor snapshots) and keep execution model pluggable.
- Risk: accidental behavior changes in existing single-loop runtime.
  - Mitigation: additive APIs only; keep default agent flow untouched and run existing targeted regression tests.
- Risk: over-scoping into full architecture migration.
  - Mitigation: explicitly defer full runtime conversion and gateway/cortex surfaces to follow-up tasks.

## Interfaces / Contracts
- `ProcessType` enum: `Channel`, `Branch`, `Worker`, `Compactor`, `Cortex`.
- `ProcessRuntimeProfile` contract for per-role prompt/turn/context/tool limits.
- `ProcessManager` with lifecycle supervision and snapshots for spawned process instances.

## ADR
- Required: architectural staging decision captured in `docs/architecture/adr-004-staged-multiprocess-runtime.md`.
