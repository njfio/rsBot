# Issue 1674 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing conformance/regression tests for summary stats,
comparative significance, report JSON contract, and invalid-input behavior.

T2: implement summary statistics and baseline-vs-candidate comparison
structures/functions in `tau-trainer`.

T3: add machine-readable report serialization helper and validation tests.

T4: run scoped fmt/clippy/tests and map C-01..C-04 to AC evidence.

## Tier Mapping

- Unit: statistical helper math and validation
- Functional: summary and comparative significance output behavior
- Regression: JSON report contract stability and invalid-input failure checks
- Conformance: C-01..C-04 tests
