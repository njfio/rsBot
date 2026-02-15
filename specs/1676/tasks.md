# Issue 1676 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add failing tests for CLI lifecycle control flags, preflight
dispatch handling, auth deny path, idempotent action behavior, and rollback
checkpoint validation.

T2: implement CLI surface for lifecycle commands and supporting paths/options.

T3: add startup preflight callback wiring for lifecycle control command mode.

T4: implement lifecycle control runtime command execution (auth, state,
audit, rollback validation).

T5: run scoped verification and map AC-1..AC-4 to C-01..C-05 evidence.

## Tier Mapping

- Unit: command selection/validation helpers
- Functional: lifecycle state transitions + rollback path behavior
- Integration: startup preflight command dispatch handling
- Regression: unauthorized action blocking and invalid rollback payload failure
- Conformance: C-01..C-05
