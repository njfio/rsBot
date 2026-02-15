# Issue 1666 Spec

Status: Implemented

Issue: `#1666`  
Milestone: `#24`  
Parent: `#1659`

## Problem Statement

The RL experience collector must remain stable under concurrent burst load, apply
retry/requeue semantics for stalled attempts, and provide measurable
backpressure/throughput signals without silently dropping rollouts.

## Scope

In scope:

- concurrent collector loop behavior in training runner/store
- timeout/reassignment retry semantics for stalled workers
- deterministic load harness metrics proving no-drop completion

Out of scope:

- distributed cross-node queue sharding
- external benchmark infrastructure
- dependency additions

## Acceptance Criteria

AC-1 (collector stability under burst):
Given burst enqueue and concurrent workers,
when collector executes,
then all rollouts reach terminal status without silent drops.

AC-2 (retry/requeue semantics):
Given stale worker attempts,
when timeout detection triggers,
then attempts transition to timeout and rollouts requeue/recover.

AC-3 (backpressure/throughput observability):
Given collector load harness execution,
when run completes,
then elapsed and throughput metrics are emitted.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Integration | Given 64 rollouts and 4 workers, when load harness runs, then succeeded count equals enqueued count with zero silent drop statuses. |
| C-02 | AC-2 | Regression | Given stale heartbeat attempts, when reassignment runs, then attempts timeout and work is recovered by later attempts. |
| C-03 | AC-3 | Functional | Given harness run, when metrics are emitted, then elapsed_ms and throughput_per_sec are present and positive. |

## Success Metrics

- collector burst harness passes deterministically
- stale attempts recover through retry/requeue path
- metrics are emitted in harness output for operational review
