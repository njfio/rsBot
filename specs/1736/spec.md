# Issue 1736 Spec

Status: Implemented

Issue: `#1736`  
Milestone: `#24`  
Parent: `#1663`

## Problem Statement

Experience collection must remain resilient when workers stall mid-batch.
Stalled attempts should be timed out and safely reassigned without losing span
history.

## Scope

In scope:

- add store-level timeout reassignment for stale worker attempts
- wire runner loop to trigger reassignment checks
- add chaos coverage for stalled worker reassignment and span preservation

Out of scope:

- distributed multi-node leader election
- external orchestration service changes
- dependency changes

## Acceptance Criteria

AC-1 (stalled worker timeout):
Given a running attempt with stale heartbeat,
when timeout reassignment runs,
then attempt is marked timeout and rollout requeues.

AC-2 (safe reassignment behavior):
Given a stalled worker and active backup worker,
when reassignment occurs,
then backup worker completes the rollout successfully.

AC-3 (no trajectory loss):
Given reassigned rollout attempts,
when querying spans,
then spans from timed-out and succeeding attempts are both retained.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given stale heartbeat, when reassignment executes, then attempt transitions to timeout and rollout transitions to requeuing. |
| C-02 | AC-2 | Integration | Given slow and fast workers, when slow worker stalls, then fast worker reassignment completes rollout with second attempt. |
| C-03 | AC-3 | Regression | Given reassigned attempts, when querying spans by rollout/attempt, then both attempts retain non-empty span history. |

## Success Metrics

- collector safely reassigns stalled work without terminal data loss
- chaos tests exercise timeout + reassignment path deterministically
