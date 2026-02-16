# Issue 1641 Spec

Status: Implemented

Issue: `#1641`  
Milestone: `#21`  
Parent: `#1616`

## Problem Statement

`#1641` is still open without issue-bound spec artifacts and parent-level
conformance evidence. Existing safety tests cover many bypass attempts, but
the runtime still allows a fail-open path when outbound payload serialization
fails before secret-leak inspection.

## Scope

In scope:

- add spec/plan/tasks artifacts for `#1641`
- enforce fail-closed behavior for outbound payload inspection when block mode
  is configured and payload serialization fails
- add regression coverage for serialization-failure bypass attempts
- document fail-closed semantics for inbound/tool-output/outbound stages
- add issue-scope conformance harness for source/tests/docs contract

Out of scope:

- redesigning safety rule sets or reason-code taxonomy beyond this issue
- adding dependencies or changing wire formats
- unrelated runtime refactors

## Acceptance Criteria

AC-1 (parent conformance contract):
Given the safety pipeline code/tests/docs,
when issue checks run,
then bypass coverage expectations for inbound/tool-output/outbound fail-closed
behavior are explicitly validated.

AC-2 (outbound fail-closed enforcement):
Given safety policy block mode for outbound secret-leak checks,
when request payload serialization fails,
then runtime returns `AgentError::SafetyViolation` instead of silently
continuing.

AC-3 (regression coverage):
Given tau-agent-core safety tests,
when targeted regression tests run,
then bypass attempts (including serialization-failure path) are blocked with
deterministic stage/reason behavior.

AC-4 (operator documentation):
Given `docs/guides/quickstart.md`,
when reviewed,
then fail-closed semantics and bypass expectations are explicit for inbound,
tool-output reinjection, and outbound payload enforcement.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given issue harness script, when run, then source/test/docs fail-closed contract tokens are present. |
| C-02 | AC-2 | Regression | Given block mode with non-finite outbound payload serialization, when prompt runs, then outbound stage fails closed with safety violation. |
| C-03 | AC-3 | Integration | Given targeted safety tests, when executed, then inbound/tool-output/outbound bypass tests pass. |
| C-04 | AC-4 | Functional | Given quickstart docs, when reviewed via harness, then fail-closed semantics section is present and stage-specific behavior is described. |

## Success Metrics

- `#1641` has explicit spec-driven closure artifacts and verification evidence
- no silent pass-through remains for outbound serialization failures in block
  mode
- operators have clear fail-closed semantics in user-facing docs
