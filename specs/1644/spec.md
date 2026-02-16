# Issue 1644 Spec

Status: Implemented

Issue: `#1644`  
Milestone: `#21`  
Parent: `#1618`

## Problem Statement

Safety diagnostics/event surfaces exist across runtime output and diagnostics
aggregation, but parent task `#1644` is still open without issue-specific
spec/plan/tasks artifacts and parent-level conformance evidence connecting
operator inspection commands, schema/version contracts, and stable reason-code
payload shapes.

## Scope

In scope:

- add `specs/1644/{spec,plan,tasks}.md`
- add issue harness for safety diagnostics source/tests/docs contract markers
- document operator-facing safety diagnostics/telemetry inspection commands with
  sample payload fields
- run targeted diagnostics/safety tests and scoped quality checks

Out of scope:

- changing runtime safety enforcement logic
- adding dependencies or protocol changes
- unrelated diagnostics refactors

## Acceptance Criteria

AC-1 (operator inspect surface):
Given quickstart/operator docs,
when reviewed,
then operators have explicit commands to inspect safety telemetry payloads and
reason codes.

AC-2 (stable diagnostics payload shape):
Given runtime event JSON mapping tests,
when targeted tests run,
then `safety_policy_applied` payload fields are stable (`type`, `stage`,
`mode`, `blocked`, `reason_codes`).

AC-3 (schema/version compatibility):
Given diagnostics schema tests,
when targeted tests run,
then v1 telemetry schema and legacy compatibility behavior remain explicit and
stable.

AC-4 (verification):
Given issue harness and scoped checks,
when run,
then source/tests/docs contract + roadmap/fmt/clippy checks pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given docs inspection commands, when reviewed via harness, then explicit safety telemetry inspection commands and sample fields are present. |
| C-02 | AC-2 | Unit | Given runtime event serialization tests, when run, then safety diagnostics JSON shape remains stable. |
| C-03 | AC-3 | Regression | Given diagnostics schema compatibility tests, when run, then v1/legacy/future-version behavior remains stable. |
| C-04 | AC-4 | Integration | Given issue commands, when run, then harness + targeted tests + roadmap/fmt/clippy pass. |

## Success Metrics

- `#1644` has complete spec-driven closure artifacts
- safety diagnostics inspect contract is explicit for operators
- diagnostics payload schema/version stability is verified and documented
