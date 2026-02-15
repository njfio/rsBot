# Issue 1726 Spec

Status: Implemented

Issue: `#1726`  
Milestone: `#24`  
Parent: `#1710`

## Problem Statement

Checkpoint persistence now exists, but M24 still needs explicit corruption
recovery and rollback safety validation focused on deterministic behavior and
operator diagnostics when restore flows degrade.

## Scope

In scope:

- add deterministic tests for corrupted primary + fallback checkpoint scenarios
- validate rollback behavior for primary-valid and fallback-required cases
- add explicit operator-facing diagnostics formatting for resume outcomes
- ensure combined failure diagnostics remain actionable

Out of scope:

- remote checkpoint storage and retention policy
- checkpoint encryption/signature handling
- control-plane command UX for resume operations (`#1676`)

## Acceptance Criteria

AC-1 (corruption recovery determinism):
Given corrupted checkpoint inputs,
when rollback resume is attempted,
then behavior is deterministic and either loads a valid fallback or returns a
combined actionable error.

AC-2 (rollback safety):
Given valid primary and fallback checkpoints,
when rollback resume executes,
then primary is preferred and fallback is not used.

AC-3 (operator diagnostics):
Given resume outcomes,
when operator diagnostics are rendered,
then source, run id, step metadata, and fallback warnings are surfaced
consistently.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Regression | Given corrupted primary and fallback checkpoints, when rollback load executes, then error contains both primary and fallback failure diagnostics. |
| C-02 | AC-2 | Integration | Given valid primary and fallback checkpoints, when rollback load executes, then source is `Primary` and diagnostics are empty. |
| C-03 | AC-3 | Functional | Given primary/fallback resume outcomes, when operator diagnostic rendering executes, then output contains deterministic source/run/step summary and fallback warning lines where applicable. |

## Success Metrics

- corruption handling paths are deterministic and covered by regression tests
- rollback source selection is explicitly validated
- operator diagnostics are actionable and stable
