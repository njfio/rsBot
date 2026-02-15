# Issue 1968 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for deterministic filename/export path, JSON
payload parity, nested directory creation, and invalid-destination failure.

T2: add export summary type and export helper in `benchmark_artifact`.

T3: add safe filename sanitization for suite/policy ids.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: invalid destination deterministic error
- Functional: deterministic filename/export success
- Conformance: persisted JSON matches in-memory artifact payload
- Integration: nested directory creation and successful export
- Regression: export helper rejects ambiguous destination cases
