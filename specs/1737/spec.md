# Issue 1737 Spec

Status: Implemented

Issue: `#1737`  
Milestone: `#24`  
Parent: `#1661`

## Problem Statement

Safety penalty coefficients need deterministic calibration so RL reward shaping
avoids reward hacking (too little penalty) and oversuppression (too much
penalty). Default coefficients must be selected from explicit experiment output.

## Scope

In scope:

- add safety-penalty calibration grid evaluator in `tau-algorithm`
- add benchmark fixture for calibration observations
- select deterministic default coefficients from calibration output
- add functional/integration/regression tests

Out of scope:

- live provider benchmark execution
- external dashboard/reporting pipelines
- dependency changes

## Acceptance Criteria

AC-1 (calibration grid):
Given candidate safety penalty coefficients with reward/safety observations,
when calibration runs,
then candidates are filtered by policy thresholds and ranked deterministically.

AC-2 (default coefficient selection):
Given calibration observations and policy bounds,
when selection runs,
then default coefficients are derived from experiment output and exposed for
runtime use.

AC-3 (regression guardrails):
Given candidate sets where none satisfy safety/reward constraints,
when selection runs,
then calibration fails closed with deterministic error reasons.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given a calibration grid, when evaluated, then only threshold-compliant candidates remain and ordering is deterministic. |
| C-02 | AC-2 | Integration | Given benchmark fixture observations, when selecting defaults, then expected coefficient is chosen and surfaced in report output. |
| C-03 | AC-3 | Regression | Given no candidate passing thresholds, when selecting defaults, then evaluator returns fail-closed error. |

## Success Metrics

- safety penalty defaults are justified by deterministic calibration output
- calibration regressions are caught by fixture-backed tests
