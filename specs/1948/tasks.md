# Issue 1948 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for epoch-expanded step accounting,
invalid-epoch validation, and numeric guardrail regressions.

T2: implement `epochs` config and epoch-aware update aggregation.

T3: implement additional numeric guardrails and deterministic failure reasons.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: config validation and guard checks
- Functional: epoch-aware update accounting
- Integration: deterministic fixture update behavior across epochs
- Regression: numeric guardrail fail-closed behavior
- Conformance: C-01..C-04
