# Issue 1992 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): add failing tests for combined report pass path,
reason-code propagation, report JSON shape, and invalid-policy failure.

T2: add combined manifest report model and builder helper.

T3: add report serialization with nested `manifest` + `quality` sections.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: invalid policy fail-closed behavior
- Functional: deterministic pass-path combined report
- Integration: failing reason-code propagation
- Conformance: machine-readable combined report JSON shape
- Regression: zero-manifest combined report stability
