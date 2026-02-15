# Issue 1696 Spec

Status: Implemented

Issue: `#1696`  
Milestone: `#24`  
Parent: `#1669`

## Problem Statement

The GAE pipeline needs explicit edge-case coverage for truncation, terminal
bootstrapping, and sparse reward sequences to prevent silent regression or
non-finite propagation.

## Scope

In scope:

- add truncation scenario tests
- add terminal bootstrap masking tests
- add sparse reward stability tests
- assert finite outputs across edge fixtures

Out of scope:

- distributed rollout semantics
- optimizer integration
- benchmark orchestration

## Acceptance Criteria

AC-1 (truncation stability):
Given truncated trajectories with non-terminal tail,
when GAE runs with bootstrap value,
then outputs remain finite and deterministic.

AC-2 (terminal masking):
Given terminal steps,
when bootstrap value is non-zero,
then terminal transitions correctly mask bootstrap contribution.

AC-3 (sparse reward resilience):
Given sparse reward trajectories,
when GAE runs,
then advantages remain finite and no NaN/inf escapes occur.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Unit | Given truncated non-terminal trajectory with bootstrap value, when GAE runs, then outputs are finite and deterministic. |
| C-02 | AC-2 | Regression | Given terminal final step and non-zero bootstrap, when GAE runs, then terminal step advantage equals reward minus value (bootstrap masked). |
| C-03 | AC-3 | Unit | Given sparse rewards across long horizon, when GAE runs, then advantages are finite and contain no NaN/inf values. |

## Success Metrics

- deterministic edge-case tests exist for truncation, terminal masking, sparse rewards
- no non-finite propagation under edge scenarios
