# Issue 1643 Spec

Status: Implemented

Issue: `#1643`  
Milestone: `#21`  
Parent: `#1616`

## Problem Statement

Outbound secret-leak fixture matrix and safety tests exist, but parent task
`#1643` remains open without issue-specific spec artifacts and parent-level
conformance evidence linking outbound block/redact behavior, reason-code
determinism, and operator validation commands.

## Scope

In scope:

- add `specs/1643/{spec,plan,tasks}.md`
- add issue harness validating outbound source/tests/docs contract markers
- document deterministic outbound validation commands for operators
- verify targeted outbound integration/regression tests and scoped quality
  checks

Out of scope:

- changing secret-leak detection rules
- dependency/protocol changes
- unrelated runtime refactors

## Acceptance Criteria

AC-1 (outbound block/redact coverage):
Given outbound safety tests,
when targeted commands run,
then block and redact behavior are deterministic across direct and fixture
matrix scenarios.

AC-2 (reason-code determinism):
Given outbound fixture regression tests,
when targeted commands run,
then stage/reason codes remain stable.

AC-3 (no silent leak path evidence):
Given outbound block-mode regression tests,
when targeted commands run,
then silent pass-through paths are rejected with explicit safety violation
behavior.

AC-4 (parent conformance + verification):
Given issue harness and scoped quality checks,
when run,
then source/tests/docs contract + roadmap/fmt/clippy are green.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given direct outbound tests and fixture matrix tests, when run, then block/redact behavior remains deterministic. |
| C-02 | AC-2 | Regression | Given outbound reason-code regression suite, when run, then stage and reason codes are stable. |
| C-03 | AC-3 | Regression | Given outbound no-silent-pass-through regression path, when run, then fail-closed safety behavior is enforced. |
| C-04 | AC-4 | Functional | Given issue harness + scoped checks, when run, then source/tests/docs markers and quality gates pass. |

## Success Metrics

- `#1643` has complete spec-driven closure artifacts
- outbound enforcement evidence is explicit and reproducible
- operator quickstart includes deterministic outbound validation commands
