# Issue 1642 Spec

Status: Implemented

Issue: `#1642`  
Milestone: `#21`  
Parent: `#1616`

## Problem Statement

Subtasks for inbound fixture corpus and tool-output reinjection regressions were
implemented, but parent task `#1642` remains open without issue-bound
spec/plan/tasks artifacts and parent-level conformance evidence tying fixture
coverage, deterministic reason-code behavior, and operator validation docs
together.

## Scope

In scope:

- create parent artifacts for `#1642` under `specs/1642/`
- add issue-scope harness checking inbound/tool-output safety coverage markers
- validate existing integration/regression tests for warn/redact/block behavior
  and stable reason codes
- document operator commands for deterministic inbound/tool-output validation

Out of scope:

- changing safety rule definitions
- adding dependencies or protocol changes
- unrelated runtime refactors

## Acceptance Criteria

AC-1 (inbound stage coverage):
Given the inbound safety fixture corpus integration tests,
when target tests run,
then warn/redact/block behavior is covered across malicious and benign cases.

AC-2 (tool-output reinjection coverage):
Given tool-output reinjection safety tests,
when target tests run,
then bypass payloads are blocked in block mode and reason/stage outputs remain
deterministic.

AC-3 (parent conformance contract):
Given issue-scope source/tests/docs checks,
when harness runs,
then fixture/test/docs contract markers for inbound/tool-output enforcement are
present.

AC-4 (verification):
Given issue-scope checks,
when run,
then harness + targeted tests + roadmap/fmt/clippy checks pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given inbound corpus tests, when run, then warn/redact/block behavior remains deterministic for malicious and benign fixture payloads. |
| C-02 | AC-2 | Regression | Given tool-output bypass cases, when run, then blocked-mode reinjection is denied and stable stage/reason expectations hold. |
| C-03 | AC-3 | Functional | Given issue harness script, when run, then expected source/test/docs contract tokens are present. |
| C-04 | AC-4 | Integration | Given issue commands, when run, then harness + targeted tests + roadmap/fmt/clippy are green. |

## Success Metrics

- `#1642` has complete spec-driven closure artifacts
- inbound and tool-output safety behavior is validated with deterministic test
  evidence
- operators have a clear validation command path for this stage coverage
