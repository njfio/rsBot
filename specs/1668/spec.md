# Issue 1668 Spec

Status: Implemented

Issue: `#1668`  
Milestone: `#24`  
Parent: `#1660`

## Problem Statement

Tau lacks a concrete PPO objective math core for policy optimization updates.
Without a deterministic clipped-surrogate loss implementation and update-step
aggregation, RL policy training cannot advance beyond scaffolding.

## Scope

In scope:

- implement PPO clipped surrogate loss computation
- include value-loss and entropy terms with configurable coefficients
- add update-step aggregation with gradient accumulation controls
- add deterministic unit vectors and regression tests

Out of scope:

- autodiff optimizer integration with external tensor runtimes
- distributed multi-node optimizer coordination
- benchmark fixture catalog expansion (covered by follow-up issue `#1695`)

## Acceptance Criteria

AC-1 (PPO clipped objective):
Given deterministic PPO sample vectors,
when loss computation runs,
then policy/value/entropy/total terms match expected reference values within
tolerance.

AC-2 (update-step aggregation):
Given a sample batch and gradient accumulation settings,
when update aggregation runs,
then minibatch grouping and optimizer-step summaries are deterministic and
correct.

AC-3 (numerical guards):
Given invalid or non-finite inputs,
when PPO math executes,
then execution fails closed with explicit errors and no NaN/inf escapes.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Unit | Given fixed sample vectors, when `compute_ppo_loss` executes, then each loss term matches expected tolerance-bounded values. |
| C-02 | AC-2 | Regression | Given fixed samples + minibatch/accumulation settings, when `compute_ppo_update` executes, then optimizer step count and aggregated losses are deterministic. |
| C-03 | AC-3 | Unit | Given non-finite/invalid samples or config, when PPO math runs, then typed validation errors are returned. |

## Success Metrics

- PPO objective math is implemented in `tau-algorithm` with deterministic tests
- update-step aggregation supports gradient accumulation semantics
- invalid numeric inputs are rejected with clear error states
