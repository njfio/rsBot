# Issue 1644 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add safety diagnostics telemetry harness and capture RED on
missing quickstart diagnostics section.

T2: add quickstart section for safety telemetry inspection commands and sample
fields.

T3: run GREEN harness and targeted runtime/diagnostics/safety tests.

T4: run roadmap/fmt/clippy checks and prepare PR closure evidence.

## Tier Mapping

- Functional: diagnostics harness source/tests/docs contract checks
- Unit: runtime `safety_policy_applied` JSON mapping test
- Regression: diagnostics schema compatibility tests
- Integration: targeted safety event emission coverage
