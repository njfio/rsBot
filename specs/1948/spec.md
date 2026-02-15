# Issue 1948 Spec

Status: Implemented

Issue: `#1948`  
Milestone: `#24`  
Parent: `#1660`

## Problem Statement

PPO update aggregation currently executes a single pass over minibatches per
update call. Story #1660 still requires explicit epoch controls and stricter
numerical guardrails for production-scale update stability.

## Scope

In scope:

- add epoch count configuration for PPO update aggregation
- extend update summaries with epoch-aware accounting
- enforce additional numeric guardrails on ratio/KL/loss outputs
- add deterministic tests for epoch accounting and guard failures

Out of scope:

- distributed optimizer scheduling
- adaptive learning-rate or optimizer-specific hyperparameter tuning
- training runtime orchestration changes outside `tau-algorithm::ppo`

## Acceptance Criteria

AC-1 (epoch controls):
Given `epochs > 1`,
when PPO update computes,
then minibatch/optimizer-step accounting scales deterministically by epoch.

AC-2 (config validation):
Given invalid epoch or guard-related values,
when validation runs,
then it fails closed with explicit field-oriented errors.

AC-3 (numeric guardrails):
Given non-finite or overflow-prone intermediate outputs,
when PPO loss/update computes,
then guardrails emit deterministic failures before returning summaries.

AC-4 (deterministic coverage):
Given fixed reference fixtures,
when update computes across epochs,
then summary metrics remain stable and reproducible.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given `epochs=3` and deterministic samples, when update runs, then minibatch and optimizer step counts equal expected epoch-expanded totals. |
| C-02 | AC-2 | Unit | Given `epochs=0` or invalid guard values, when validation executes, then deterministic error text references offending fields. |
| C-03 | AC-3 | Regression | Given synthetic unstable sample values, when update runs, then guardrail failure prevents invalid summary output. |
| C-04 | AC-4 | Integration | Given reference fixture cases, when epoch-aware updates run, then summary outputs are deterministic across repeated executions. |

## Success Metrics

- PPO update surface includes explicit epoch controls
- numerical guardrails fail closed on unstable updates
- deterministic tests prevent accounting/guardrail regressions
