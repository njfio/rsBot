# Spec #2042

Status: Implemented
Milestone: specs/milestones/m25/index.md
Issue: https://github.com/njfio/Tau/issues/2042

## Problem Statement

`crates/tau-tools/src/tools.rs` exceeded the M25 decomposition threshold and
required phased extraction below 3000 LOC while preserving tool behavior and
policy gate contracts used by runtime callers.

## Acceptance Criteria

- AC-1: `tools.rs` is reduced below 3000 LOC according to the approved split
  map.
- AC-2: Tool/policy behavior remains stable after extraction.
- AC-3: Unit/functional/integration/regression evidence is posted for the
  decomposition wave.

## Scope

In:

- Execute split-map planning subtask `#2062`.
- Execute runtime extraction/parity subtask `#2063`.
- Capture threshold and parity evidence.

Out:

- Runtime decomposition tasks outside `tools.rs`.

## Conformance Cases

- C-01 (AC-1): `tools.rs` line count is below 3000 and split guardrail passes.
- C-02 (AC-2): extracted module wiring preserves tool export/policy markers.
- C-03 (AC-3): integration/contract suite remains green post-split.

## Success Metrics

- `tools.rs` maintained under threshold with split-map and execution subtasks
  closed.
