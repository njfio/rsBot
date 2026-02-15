# Issue 1954 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for default compatibility, tail truncation,
padding to fixed window, and invalid policy.

T2: add window policy types and validation path in `SpansToTrajectories`.

T3: implement deterministic truncate/pad transform and reindex rules.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: invalid policy fail-closed
- Functional: default compatibility behavior
- Conformance: truncation and padding semantics
- Regression: deterministic step indexing/done semantics after transform
