# Issue 1670 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing conformance tests for checkpoint roundtrip,
corruption fallback resume, and unsupported-version rejection.

T2: implement checkpoint save/load APIs and payload validation.

T3: implement rollback-aware resume load path with explicit diagnostics.

T4: run scoped fmt/clippy/tests and map AC-1..AC-3 to C-01..C-03 evidence.

## Tier Mapping

- Unit: checkpoint schema validation helpers
- Integration: checkpoint save/load roundtrip flow
- Regression: corruption fallback and unsupported-version errors
- Conformance: C-01..C-03
