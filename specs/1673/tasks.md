# Issue 1673 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing conformance tests for deterministic run behavior,
repeatability tolerance checks, and malformed fixture failure-path validation.

T2: implement benchmark scorer trait + benchmark run report model and suite
driver in `tau-trainer`.

T3: implement repeatability evaluator and case-level drift reporting.

T4: run scoped fmt/clippy/tests and map AC-1..AC-3 with C-01..C-03 evidence.

## Tier Mapping

- Unit: repeatability delta/range logic
- Functional: deterministic suite execution behavior
- Regression: malformed fixture failure-path checks
- Conformance: C-01..C-03
