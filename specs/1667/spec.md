# Issue 1667 Spec

Status: Implemented

Issue: `#1667`  
Milestone: `#24`  
Parent: `#1663`

## Problem Statement

Worker heartbeat data must accurately distinguish stalled workers from healthy,
long-running workers. If heartbeat refresh does not propagate to active
attempts, timeout recovery can incorrectly reassign healthy work.

## Scope

In scope:

- keep attempt heartbeat fresh when worker heartbeat updates include active
  rollout/attempt IDs
- preserve timeout-reassignment behavior for truly stale attempts
- validate operator-visible status via worker and attempt queries

Out of scope:

- cross-node leader election
- new dependency adoption
- CLI/UX redesign for operator dashboards

## Acceptance Criteria

AC-1 (stale detection and recovery):
Given a running attempt with stale heartbeat,
when timeout reassignment runs,
then the attempt transitions to timeout and the rollout requeues.

AC-2 (healthy long-run protection):
Given a long-running attempt with periodic heartbeat refresh,
when timeout reassignment runs concurrently,
then the attempt is not falsely timed out.

AC-3 (operator-visible correctness):
Given timeout recovery and healthy execution paths,
when querying workers and attempts,
then active/idle and terminal statuses are accurate.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given stale heartbeat, when reassignment executes, then attempt becomes timeout and rollout requeues. |
| C-02 | AC-2 | Regression | Given periodic active heartbeat during long execution, when reassignment ticks, then no timeout/requeue occurs. |
| C-03 | AC-3 | Functional | Given timeout and success paths, when querying workers/attempts, then reported states match runtime truth. |

## Success Metrics

- no false timeouts for healthy long-running attempts under configured heartbeat
- stale attempts still recover through timeout + reassignment path
- worker/attempt query outputs remain consistent with execution outcomes
