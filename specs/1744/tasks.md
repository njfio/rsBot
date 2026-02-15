# Issue 1744 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for lifecycle control audit schema
conformance and serve transition audit emission.

T2: implement runtime lifecycle control audit schema + audit-enabled serve path.

T3: add diagnostics compliance accounting for lifecycle control audit records.

T4: run fmt/clippy/tests for touched crates and verify AC mapping.

## Tier Mapping

- Unit: lifecycle schema/action validation
- Functional: serve lifecycle control transition audit emission
- Regression: malformed lifecycle records compliance accounting
