# Issue 1972 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for deterministic entry ordering,
mixed valid/invalid handling, manifest JSON projection, and missing-directory
errors.

T2: add manifest types and directory scan helper in `benchmark_artifact`.

T3: add deterministic path sorting and invalid-file diagnostics.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: missing-directory deterministic error
- Functional: deterministic entry ordering
- Integration: mixed valid/invalid scan behavior
- Conformance: machine-readable manifest JSON fields
- Regression: non-JSON files are ignored
