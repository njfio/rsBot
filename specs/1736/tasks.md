# Issue 1736 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing integration/regression chaos tests for stale
worker timeout reassignment and span preservation.

T2: implement timeout reassignment operation in training store backends.

T3: wire runner reassignment tick + stale-attempt completion guard.

T4: run fmt/clippy/tests for touched crates and verify AC mapping.

## Tier Mapping

- Integration: stale worker timeout + reassignment completion
- Regression: retained spans across timed-out and succeeding attempts
