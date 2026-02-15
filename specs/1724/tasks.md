# Issue 1724 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing migration/version tests for legacy fixtures and
unknown schema versions.

T2: implement any required schema/docs adjustments to satisfy tests.

T3: run scoped fmt/clippy/tests and verify AC mapping.

## Tier Mapping

- Unit: legacy fixture decode + validation success
- Functional: documented migration guarantees in code/tests
- Regression: unknown schema version fail-closed behavior
