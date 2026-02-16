# Issue 1976 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): add failing tests for combined report pass path, reason-code
propagation, report JSON shape, and invalid-policy failure.

T2: add gate report model and builder helper.

T3: wire report serialization with nested manifest+quality sections.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: invalid policy fail-closed behavior
- Functional: deterministic pass-path report
- Integration: failing reason-code propagation
- Conformance: machine-readable report JSON shape
- Regression: zero-scan manifest report remains stable
