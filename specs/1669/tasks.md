# Issue 1669 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing deterministic GAE conformance tests for known
vectors, normalization/clipping behavior, and invalid edge cases.

T2: implement GAE core computation and config parsing in `tau-algorithm`.

T3: implement trajectory-to-`AdvantageBatch` conversion with missing
value-estimate guards.

T4: run fmt/clippy/tests for touched crates and map ACs to passing cases.

## Tier Mapping

- Unit: known-vector conformance
- Functional: normalization/clipping controls
- Regression: invalid-input fail-closed behavior
