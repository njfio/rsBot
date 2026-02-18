# M77 - Spacebot G7 Memory Lifecycle (Phase 2)

Milestone: GitHub milestone `M77 - Spacebot G7 Memory Lifecycle (Phase 2)`

## Objective

Deliver the second production slice of `tasks/spacebot-comparison.md` gap `G7`
by adding deterministic decay, prune, and orphan-cleanup maintenance behavior to
Tau memory runtime.

## Scope

- Add lifecycle maintenance policy and execution API to `tau-memory`.
- Decay stale memory importance for non-identity records.
- Prune records below configurable importance floor using soft-delete
  (`forgotten=true`).
- Clean orphan low-importance memories (no inbound/outbound graph edges) using
  soft-delete.
- Add conformance + regression tests for decay/prune/orphan behavior.

## Out of Scope

- Heartbeat scheduler wiring for automatic periodic execution.
- Near-duplicate embedding merge pipeline.
- UI visualization for lifecycle maintenance state.

## Linked Hierarchy

- Epic: #2453
- Story: #2454
- Task: #2455
- Subtask: #2456
