# Issue 1738 Spec

Status: Implemented

Issue: `#1738`  
Milestone: `#24`  
Parent: `#1661`

## Problem Statement

Checkpoint promotion must fail closed when candidate safety metrics regress past
an explicit threshold. Promotion decisions also need structured logs for
operator auditability.

## Scope

In scope:

- define checkpoint promotion threshold policy in `tau-trainer`
- implement promotion gate decision evaluation
- add runtime audit payload for promotion decisions in `tau-runtime`
- add unit/integration/regression tests for gate behavior and logging

Out of scope:

- full checkpoint registry/persistence service
- external dashboard UI changes
- dependency changes

## Acceptance Criteria

AC-1 (threshold policy):
Given baseline and candidate safety metrics plus policy thresholds,
when promotion gate evaluates,
then checkpoints are blocked when safety regression exceeds policy.

AC-2 (promotion gate integration):
Given significance reproducibility outputs and threshold policy,
when gate evaluates promotion,
then promotion requires both safety threshold and reproducibility gates.

AC-3 (decision logging):
Given promotion gate decisions,
when runtime audit payload is generated,
then payload includes threshold values, computed regression, decision, and
reason codes.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Unit | Given candidate safety regression above threshold, when gate evaluates, then promotion is denied with deterministic reason code. |
| C-02 | AC-2 | Integration | Given significance reports and safety metrics within thresholds, when gate evaluates, then promotion is allowed only when all required gates pass. |
| C-03 | AC-3 | Regression | Given denied decisions, when audit payload is built, then computed regression/threshold and reason codes remain stable. |

## Success Metrics

- no unsafe checkpoint promotions when regression exceeds configured threshold
- audit payloads encode promotion decisions with deterministic reason codes
