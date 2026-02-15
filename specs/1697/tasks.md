# Issue 1697 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing conformance/regression tests for fixture family
coverage, rubric normalization, and malformed-fixture rejection.

T2: add reproducible reasoning/tool-use benchmark fixture files and README.

T3: implement `tau-trainer` fixture loader/validator with deterministic error
contracts.

T4: run scoped fmt/clippy/tests and map AC-1..AC-3 to passing conformance
cases.

## Tier Mapping

- Unit: fixture parsing and field-level validation
- Functional: fixture family coverage and deterministic seed/case checks
- Conformance: C-01 and C-02 loader/rubric contract assertions
- Regression: C-03 malformed fixture rejection assertions
