# Issue 1960 Spec

Status: Implemented

Issue: `#1960`  
Milestone: `#24`  
Parent: `#1659`

## Problem Statement

Rollout retry/requeue paths create multiple attempts and span streams, but there is
no deterministic helper to audit persistence integrity for a rollout id. Operators
cannot quickly prove whether data was dropped across retries.

## Scope

In scope:

- add rollout persistence audit helper using existing `TrainingStore` methods
- summarize rollout status, expected attempts, attempt statuses, and span counts
- detect deterministic persistence gaps (missing attempts, missing spans on terminal attempts)

Out of scope:

- storage schema changes
- dashboard/CLI surfaces
- mutation of rollout data (read-only audit)

## Acceptance Criteria

AC-1 (deterministic audit summary):
Given a rollout id,
when audit helper runs,
then it returns deterministic summary with attempt/span accounting.

AC-2 (retry/requeue integrity coverage):
Given retry/requeue rollout history,
when audit helper runs,
then it reports all attempts and confirms no persistence gaps.

AC-3 (gap detection):
Given missing attempt record or missing terminal attempt spans,
when audit helper runs,
then it marks persistence gaps with deterministic reason strings.

AC-4 (error determinism):
Given unknown rollout id,
when audit helper runs,
then it fails with deterministic not-found error.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given a single succeeded rollout, when audit runs, then attempt/span counts are deterministic and gap-free. |
| C-02 | AC-2 | Integration | Given a reassigned timeout->success rollout, when audit runs, then two attempts with persisted spans are reported and gap-free. |
| C-03 | AC-3 | Conformance | Given a store wrapper that hides one attempt record, when audit runs, then gap reason includes missing attempt record. |
| C-04 | AC-4 | Unit | Given unknown rollout id, when audit runs, then deterministic not-found error is returned. |

## Success Metrics

- rollout persistence integrity can be validated with one helper call
- retry/requeue no-data-loss proof is reproducible in integration tests
- gap reasons are deterministic and machine-readable
