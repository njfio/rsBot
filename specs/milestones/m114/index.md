# M114 - Spacebot G1 Multi-Process Architecture (Phase 2)

Status: Active

## Context
`tasks/spacebot-comparison.md` still lists unchecked `G1` architectural items even though staged primitives (`ProcessType`, `ProcessRuntimeProfile`, `ProcessManager`) exist in `tau-agent-core`. The remaining gap is wiring those primitives into live turn execution so delegation paths are supervised and role-specific runtime limits are enforced.

## Source
- `tasks/spacebot-comparison.md` (G1 multi-process architecture)

## Objective
Integrate staged process contracts into branch/worker execution paths so channel turns can delegate via supervised process lineage and worker runs execute in isolated tokio tasks with role-specific limits.

## Scope
- Apply process runtime profiles in delegated branch/worker follow-up execution.
- Supervise delegated process lifecycles with `ProcessManager`.
- Emit deterministic delegation metadata in branch tool results for downstream verification.
- Add conformance/regression tests for process lineage and lifecycle behavior.

## Issue Map
- Epic: #2719
- Story: #2720
- Task: #2721

## Acceptance Signals
- Branch follow-up path registers channel -> branch -> worker lineage snapshots.
- Worker follow-up runs under `ProcessType::Worker` runtime profile (`max_turns=25`) and isolated tokio task execution.
- Existing branch-tool behavior remains stable for limits and failure paths.
