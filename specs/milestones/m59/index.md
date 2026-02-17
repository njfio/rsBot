# M59 — Context Compaction Thresholds (G2 Slice)

Milestone: [GitHub milestone #59](https://github.com/njfio/Tau/milestone/59)

## Objective

Implement tiered context compaction thresholds in `tau-agent-core` so long
conversations degrade gracefully before token budget overflow.

## Scope

- Add threshold-driven compaction decisions based on estimated input-token
  utilization.
- Support warn/aggressive/emergency compaction tiers with deterministic
  behavior.
- Validate with conformance/regression/integration tests.

## Out of Scope

- LLM-based background summarization workers.
- Cross-process compactor architecture or Cortex observer system.

## Linked Hierarchy

- Epic: #2359
- Story: #2360
- Task: #2361
- Subtask: #2362
