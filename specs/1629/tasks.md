# Issue 1629 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add `scripts/dev/test-memory-backend-disposition.sh` with explicit unsupported-postgres docs expectation and run RED.

T2: update `docs/guides/memory-ops.md` to explicitly state postgres is unsupported and falls back with invalid-backend reason code.

T3: run GREEN conformance harness and targeted tau-memory regression test.

T4: run scoped fmt/clippy checks and prepare PR evidence mapping AC -> tests.

## Tier Mapping

- Unit: existing tau-memory unit/regression coverage for backend resolution
- Functional: conformance harness checks resolver/documentation contract
- Integration: targeted tau-memory regression command
- Regression: explicit postgres invalid-backend fallback test
