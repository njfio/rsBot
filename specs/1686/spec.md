# Issue 1686 Spec

Status: Implemented

Issue: `#1686`  
Milestone: `#21`  
Parent: `#1638`

## Problem Statement

`crates/tau-memory/src/runtime.rs` currently combines storage-backend resolution,
persistence operations, ranking/embedding primitives, and query orchestration in
one file. The mixed concerns reduce maintainability and increase regression risk
for memory behavior changes.

## Scope

In scope:

- split backend selection/persistence into dedicated module(s)
- split ranking/embedding logic into dedicated module(s)
- split query orchestration into dedicated module(s)
- preserve deterministic memory behavior and reason-code contracts

Out of scope:

- search algorithm behavior changes
- backend policy changes
- dependency or wire-format changes

## Acceptance Criteria

AC-1 (backend modularization):
Given memory backend selection and persistence responsibilities,
when reviewing module layout,
then backend logic is implemented in dedicated backend module(s), not inline in
the monolithic runtime file.

AC-2 (ranking modularization):
Given ranking and embedding primitives,
when reviewing module layout,
then ranking/embedding responsibilities are implemented in dedicated ranking
module(s) with existing function contracts preserved.

AC-3 (query modularization):
Given query/search/tree orchestration responsibilities,
when reviewing module layout,
then query orchestration is implemented in dedicated query module(s) with
existing public API behavior preserved.

AC-4 (behavior parity):
Given existing memory runtime tests,
when running scoped checks,
then deterministic behavior and reason-code expectations remain unchanged.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given source layout, when inspected, then backend functions live under `crates/tau-memory/src/runtime/backend.rs`. |
| C-02 | AC-2 | Functional | Given source layout, when inspected, then ranking/embedding functions live under `crates/tau-memory/src/runtime/ranking.rs`. |
| C-03 | AC-3 | Integration | Given memory search/tree flows, when running `tau-memory` tests, then search outputs and tree aggregation behavior remain stable. |
| C-04 | AC-4 | Regression | Given scoped checks, when running crate tests + strict clippy + fmt + split harness, then all pass without behavior drift. |

## Success Metrics

- `runtime.rs` reduced to composition/API surface
- backend, ranking, and query concerns are explicitly separated
- `tau-memory` tests pass unchanged with no new warnings
