# Issue 1952 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing lineage resolver tests for success path, duplicate
id, missing parent, cycle, and unknown leaf.

T2: implement `CheckpointLineageError` and lineage resolver helper.

T3: validate resolver behavior and deterministic error text.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: unknown leaf and typed error variants
- Functional: valid root->leaf lineage path resolution
- Regression: duplicate/missing/cycle fail-closed paths
- Conformance: C-01..C-04
