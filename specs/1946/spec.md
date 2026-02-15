# Issue 1946 Spec

Status: Implemented

Issue: `#1946`  
Milestone: `#24`  
Parent: `#1660`

## Problem Statement

Current PPO utilities provide clip/value/entropy terms but no KL-target controls.
Without KL guardrails, updates can drift too far from the reference policy with
no deterministic early-stop signal.

## Scope

In scope:

- extend PPO config with KL penalty/threshold controls
- compute approximate KL divergence in PPO loss/update summaries
- apply optional KL penalty term in total loss
- emit deterministic early-stop decision when KL exceeds configured max
- add tests for config validation, KL penalty, and early-stop behavior

Out of scope:

- optimizer scheduler integration beyond PPO loss/update math utilities
- distributed trainer orchestration changes
- dashboard/reporting schema changes outside PPO structs

## Acceptance Criteria

AC-1 (configurable KL controls):
Given PPO config with KL fields,
when config validates,
then finite/non-negative constraints are enforced with explicit errors.

AC-2 (KL penalty in loss):
Given PPO samples and a non-zero KL penalty coefficient,
when loss computes,
then loss breakdown includes approximate KL and total loss includes
`kl_penalty_coefficient * approx_kl`.

AC-3 (early-stop signal):
Given PPO update summaries and `max_kl` threshold,
when mean KL exceeds threshold,
then summary marks early-stop triggered with deterministic reason.

AC-4 (deterministic coverage):
Given fixed reference fixtures and synthetic high-divergence samples,
when tests run,
then KL metrics and stop decisions are stable and reproducible.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Unit | Given invalid KL config values (negative/NaN), when validation runs, then deterministic error text references KL field names. |
| C-02 | AC-2 | Functional | Given fixed PPO samples with KL penalty configured, when loss computes, then total loss reflects KL penalty term and approx_kl is finite. |
| C-03 | AC-3 | Regression | Given high-divergence sample batches and low `max_kl`, when update computes, then early-stop flag/reason is set. |
| C-04 | AC-4 | Integration | Given deterministic fixture vectors, when update computes with KL controls, then KL metrics and step summaries remain stable. |

## Success Metrics

- PPO outputs expose explicit KL metrics for guard decisions
- KL thresholds can fail closed before unstable updates proceed
- deterministic tests prevent KL-control regressions
