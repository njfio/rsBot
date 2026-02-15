# Issue 1695 Spec

Status: Implemented

Issue: `#1695`  
Milestone: `#24`  
Parent: `#1668`

## Problem Statement

PPO math needs fixture-backed conformance coverage so future changes can be
validated against deterministic reference vectors with explicit tolerance
thresholds.

## Scope

In scope:

- add deterministic PPO reference fixture vectors in `tau-algorithm/testdata`
- add tolerance thresholds per fixture case
- validate clipped objective outputs and update-step summaries against fixtures

Out of scope:

- cross-framework gradient autodiff parity
- large benchmark dataset generation
- distributed optimizer integration

## Acceptance Criteria

AC-1 (fixture conformance):
Given reference PPO fixtures,
when conformance tests run,
then computed loss terms match expected fixture values within configured
tolerance.

AC-2 (tolerance thresholds):
Given fixture cases with explicit tolerance bounds,
when assertions run,
then failures trigger only when observed drift exceeds tolerance.

AC-3 (stability regression):
Given canonical clipping/aggregation fixture ranges,
when regression tests run,
then clipped-objective outputs remain stable and finite.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Unit | Given fixture vectors, when PPO loss is computed, then policy/value/entropy/total/ratio/clipping values match within tolerance. |
| C-02 | AC-2 | Regression | Given fixture case tolerance, when expected drift is below threshold, then test passes deterministically; above threshold would fail. |
| C-03 | AC-3 | Unit | Given clipping-edge fixture samples, when PPO update runs, then losses/step summaries are finite and deterministic. |

## Success Metrics

- fixture file exists with deterministic vectors and expected outputs
- conformance tests verify all fixture cases with explicit tolerances
- regression stability is enforced for clipped objective outputs
