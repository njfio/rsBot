# Issue 1970 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for valid replay, malformed JSON errors,
missing-key validation, and unsupported schema version.

T2: add replay validator helper in `benchmark_artifact`.

T3: enforce deterministic required-key and schema-version checks.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: malformed JSON and missing-key errors
- Functional: valid artifact replay path
- Conformance: required-key validation behavior
- Integration: unsupported schema-version behavior
- Regression: validator rejects non-object payloads
