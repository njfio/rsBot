# Spec #2442 - G6 memory graph relations phase-1 orchestration

Status: Implemented
Milestone: specs/milestones/m75/index.md
Issue: https://github.com/njfio/Tau/issues/2442

## Problem Statement

Gap `G6` in `tasks/spacebot-comparison.md` required a milestone-level delivery
slice for relation edge persistence and graph-aware memory retrieval in Tau.
This epic tracks that orchestration outcome across story/task/subtask issues in
M75.

## Scope

In scope:

- Orchestrate M75 delivery for G6 phase-1 (relation persistence + graph signal).
- Ensure work is decomposed into story/task/subtask with spec-driven artifacts.
- Verify merged implementation satisfies conformance and quality gates.

Out of scope:

- G7 lifecycle features (decay/prune/dedup/orphan cleanup).
- UI graph visualization (`G19`).
- Cortex/process architecture work (`G1`-`G4`).

## Acceptance Criteria

- AC-1: Given M75 G6 scope, when implementation is complete, then relation edge
  persistence and retrieval metadata are delivered in production code.
- AC-2: Given memory search ranking, when related records participate, then
  graph signal contributes to deterministic final rank ordering.
- AC-3: Given issue hierarchy governance requirements, when epic closes, then
  story/task/subtask artifacts and validation evidence are linked and complete.

## Conformance Cases

- C-01 (AC-1, orchestration): merged task implementation in PR #2446 includes
  relation write/read/search behavior from spec #2444.
- C-02 (AC-2, orchestration): conformance tests for #2444 include graph ranking
  behavior and pass in merged branch.
- C-03 (AC-3, governance): issues #2442/#2443/#2444/#2445 contain milestone and
  spec artifact linkage with completed status.

## Success Metrics / Observable Signals

- PR #2446 merged on `master`.
- Issue #2444 closed as delivered implementation.
- Spec lifecycle artifacts for epic/story/subtask present in `specs/`.
