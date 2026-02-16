# Issue 1974 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for pass case, zero-valid failure,
invalid-ratio failure, and decision JSON payload projection.

T2: add manifest quality policy + decision models and evaluator helper.

T3: enforce deterministic reason-code ordering and ratio computation.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: no-valid-artifacts fail path
- Functional: policy pass path
- Integration: invalid-ratio fail path
- Conformance: decision JSON payload
- Regression: invalid ratio handles zero scanned files without panic
