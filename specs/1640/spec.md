# Issue 1640 Spec

Status: Implemented

Issue: `#1640`  
Milestone: `#21`  
Parent: `#1616`

## Problem Statement

Safety-policy precedence was centralized under subtask `#1715`, but parent task `#1640` remains open without an issue-bound conformance contract tying code, tests, and operator-facing startup documentation together.

## Scope

In scope:

- codify startup safety-policy precedence contract for `profile -> cli/env -> runtime env overrides`
- add tests-first harness validating precedence constants/resolver wiring and docs presence
- document precedence explicitly in startup DI guide
- run targeted tau-startup precedence tests and scoped quality checks

Out of scope:

- changing safety policy behavior beyond documented precedence
- adding dependencies
- unrelated startup dispatch refactors

## Acceptance Criteria

AC-1 (single precedence contract):
Given `crates/tau-startup/src/startup_safety_policy.rs`,
when reviewed,
then a canonical precedence chain exists and is returned by the resolver bundle.

AC-2 (docs alignment):
Given `docs/guides/startup-di-pipeline.md`,
when reviewed,
then it explicitly documents startup safety-policy precedence layers.

AC-3 (regression coverage):
Given tau-startup precedence tests,
when run,
then cli/env/preset precedence behavior remains deterministic.

AC-4 (verification):
Given issue-scope checks,
when run,
then conformance harness + targeted tests + quality checks pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given startup safety policy source, when checked by harness, then canonical precedence constant and resolver functions are present. |
| C-02 | AC-2 | Functional | Given startup DI guide, when checked by harness, then explicit precedence section and layer order tokens are present. |
| C-03 | AC-3 | Regression | Given targeted tau-startup precedence tests, when run, then precedence invariants pass. |
| C-04 | AC-4 | Integration | Given issue-scope commands, when run, then harness + targeted tests + roadmap/fmt/clippy pass. |

## Success Metrics

- parent task has explicit code+docs+test conformance evidence
- precedence behavior remains deterministic and visible to operators/contributors
