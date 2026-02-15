# Issue 1952 Spec

Status: Implemented

Issue: `#1952`  
Milestone: `#24`  
Parent: `#1658`

## Problem Statement

`CheckpointRecord` provides per-record metadata but no deterministic helper for
building/querying lineage paths. Operators and validators cannot reliably
derive root-to-leaf chains or detect malformed lineage topologies.

## Scope

In scope:

- add checkpoint lineage resolver over `CheckpointRecord` sets
- use `metadata.parent_checkpoint_id` as lineage link key
- return deterministic root->leaf checkpoint id path
- fail closed on duplicate ids, missing parent links, and cycles

Out of scope:

- schema changes to `CheckpointRecord`
- persistence backend/query engine changes
- trainer runtime checkpoint orchestration changes

## Acceptance Criteria

AC-1 (lineage path query):
Given valid checkpoint records and a leaf checkpoint id,
when lineage resolve runs,
then it returns a deterministic root->leaf id path.

AC-2 (duplicate detection):
Given duplicate checkpoint ids in the record set,
when resolve runs,
then it fails with deterministic duplicate-id error.

AC-3 (missing/cycle fail closed):
Given missing parent links or lineage cycles,
when resolve runs,
then it fails with deterministic missing-parent or cycle errors.

AC-4 (leaf lookup validation):
Given unknown leaf checkpoint id,
when resolve runs,
then it fails with deterministic unknown-leaf error.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given a valid three-node lineage, when resolve runs for the leaf, then output path is `[root, mid, leaf]`. |
| C-02 | AC-2 | Regression | Given duplicate checkpoint ids, when resolve runs, then duplicate-id error is returned. |
| C-03 | AC-3 | Regression | Given a missing parent id and a cycle case, when resolve runs, then deterministic missing-parent and cycle errors are returned. |
| C-04 | AC-4 | Unit | Given unknown leaf id, when resolve runs, then unknown-leaf error is returned. |

## Success Metrics

- checkpoint lineage is queryable through one deterministic API
- malformed lineage topologies fail before runtime consumption
- tests lock deterministic failure reasons
