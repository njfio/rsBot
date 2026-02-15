# Issue 1966 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for deterministic artifact fields, schema
serialization, reason-code preservation, and optional-section nullability.

T2: add artifact struct + builder in `tau-trainer`.

T3: add machine-readable JSON projection with explicit schema version and
metadata fields.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: optional-section null serialization
- Functional: deterministic typed artifact construction
- Integration: promotion-gate reason-code preservation in artifact bundle
- Conformance: schema/version JSON projection
- Regression: invalid metadata validation rejected
