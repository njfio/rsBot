# Issue 1739 Spec

Status: Implemented

Issue: `#1739`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Significance reports are only actionable if conclusions are reproducible across
seeded reruns and robust to sample-size changes. Current code lacks explicit
reproducibility checks and sensitivity-band tests.

## Scope

In scope:

- add reproducibility evaluation helpers in `tau-trainer`
- add tests for repeated seeded runs
- add tests for sample-size sensitivity bands
- document interpretation limits in training operations guide

Out of scope:

- end-to-end benchmark execution engine
- statistical-model redesign beyond deterministic guardrails

## Acceptance Criteria

AC-1 (seeded reproducibility):
Given repeated seeded significance observations at fixed sample size,
when evaluated,
then report indicates whether p-value/effect ranges stay within configured bands.

AC-2 (sample-size sensitivity):
Given observations across increasing sample sizes,
when evaluated,
then report indicates whether adjacent drift remains within configured bands.

AC-3 (regression detection):
Given out-of-band seeded or sample-size observations,
when evaluated,
then deterministic failure flags are produced.

AC-4 (interpretation limits documentation):
Given training operations docs,
when reviewed,
then interpretation limits and non-causal caveats are documented.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given fixed-sample repeated seeds, when evaluated, then within-band report is true when ranges are bounded. |
| C-02 | AC-2 | Functional | Given same-seed multiple sample sizes, when evaluated, then sensitivity drift is computed and band-checked. |
| C-03 | AC-3 | Regression | Given out-of-band ranges/drift, when evaluated, then within-band report is false deterministically. |
| C-04 | AC-4 | Functional | Given docs update, when read, then interpretation limits are explicitly listed. |

## Success Metrics

- reproducibility and sensitivity checks become deterministic guardrails in M24 workflows
