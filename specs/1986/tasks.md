# Issue 1986 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): add failing tests for deterministic summary gate report
export, validator pass path, malformed/non-object validator rejection, and
file-destination export failure.

T2: add summary gate report export helper with deterministic filename and
summary output.

T3: add replay validator helper for exported summary gate report payloads.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: malformed/non-object validator rejection behavior
- Functional: deterministic summary gate report export path and summary
- Conformance: validator accepts exported payload with required sections
- Regression: export rejects file destination path
