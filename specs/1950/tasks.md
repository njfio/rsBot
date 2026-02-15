# Issue 1950 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing bundle-validation tests for id mismatch,
length mismatch, checkpoint progression mismatch, and valid pass case.

T2: implement `RlPayloadBundle` and `RlBundleError` in `tau-training-types`.

T3: implement `RlPayloadBundle::validate()` with deterministic mismatch errors.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: valid bundle pass and typed error variants
- Functional: bundle validator executes component + cross checks
- Regression: id/length/progression mismatch paths
- Conformance: C-01..C-04
