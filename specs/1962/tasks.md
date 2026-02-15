# Issue 1962 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for aggregate clean proof, retry/requeue proof,
gap propagation, and JSON artifact projection.

T2: add collector proof types and aggregation helper.

T3: add deterministic JSON projection method.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: JSON artifact projection
- Functional: single-rollout aggregate proof
- Integration: retry/requeue aggregate proof
- Conformance: gap propagation
- Regression: runner/trainer suites remain green
