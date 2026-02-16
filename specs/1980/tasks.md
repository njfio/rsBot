# Issue 1980 Tasks

Status: Implementing

## Ordered Tasks

T1 (tests-first): add failing tests for deterministic summary totals, invalid
file diagnostics, JSON serialization shape, and missing-directory error path.

T2: add gate report summary models and directory scan helper.

T3: add summary JSON projection helper.

T4: run scoped verification and map AC-1..AC-4 to C-01..C-04.

## Tier Mapping

- Unit: missing-directory fail-closed behavior
- Functional: deterministic sorted summary totals
- Integration: malformed file diagnostics while continuing scan
- Conformance: machine-readable summary JSON payload
- Regression: non-json files ignored during summary scans
