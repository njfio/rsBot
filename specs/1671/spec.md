# Issue 1671 Spec

Status: Implemented

Issue: `#1671`  
Milestone: `#24`  
Parent: `#1661`

## Problem Statement

Tau emits safety-policy telemetry (`agent.safety_policy_applied`) during rollout
execution, but RL rewards are currently shaped without those safety signals.
Unsafe trajectories can still retain positive task rewards, which weakens
safety-constrained learning.

## Scope

In scope:

- map safety reason codes into deterministic reward penalties
- add hard-gate behavior for severe safety reason codes
- make safety reward shaping/gating configurable from training config
- add adversarial and regression tests proving unsafe trajectories cannot retain
  positive reward improvements under gate conditions

Out of scope:

- changing tau-safety detection rules or reason-code taxonomy
- PPO optimizer math changes beyond reward shaping inputs
- benchmark publication/report-template schema changes

## Acceptance Criteria

AC-1 (reason-code penalty mapping):
Given rollout traces containing safety reason codes,
when safety reward shaping runs,
then deterministic penalties are applied using configured or default mappings.

AC-2 (hard gate on severe violations):
Given safety reason codes configured as hard-gate conditions,
when shaping runs for that trajectory,
then positive reward improvements are clamped/blocked and a hard-gate penalty
signal is emitted.

AC-3 (configurable safety constraints):
Given a training config safety-reward policy block,
when runtime wiring builds the executor,
then policy overrides are validated and applied; invalid values fail closed.

AC-4 (adversarial coverage):
Given adversarial safety events (prompt-injection and secret-leak style reason
codes),
when tests execute,
then reward shaping and hard-gate outcomes are deterministic and stable.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Unit | Given safety spans with mapped/unmapped reason codes, when reward shaping runs, then penalty totals match deterministic mapping rules. |
| C-02 | AC-2 | Regression | Given spans including hard-gate reason codes, when shaping runs, then positive rewards are clamped and hard-gate penalty/marker rewards are emitted. |
| C-03 | AC-3 | Unit | Given runtime config overrides with invalid negative/NaN penalty values, when policy is built, then runtime returns explicit validation errors. |
| C-04 | AC-4 | Integration | Given adversarial reason-code spans from prompt-injection/secret-leak families, when executor reward shaping runs, then unsafe trajectories do not retain positive improvement. |

## Success Metrics

- safety events deterministically alter RL reward shaping
- severe safety violations fail closed for reward improvement paths
- operators can tune safety penalties/gates without code edits
- adversarial regressions remain covered by stable tests
