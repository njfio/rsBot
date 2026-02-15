# Issue 1960 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for unknown rollout error, single-run audit,
retry/requeue audit, and missing-attempt gap detection.

T2: add audit report types + helper function.

T3: implement deterministic gap checks and reason generation.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: unknown rollout deterministic error
- Functional: single-rollout deterministic summary
- Integration: retry/requeue audit integrity
- Conformance: missing-attempt gap reason detection
- Regression: runner/trainer suites remain green
