# Issue 1962 Spec

Status: Implemented

Issue: `#1962`  
Milestone: `#24`  
Parent: `#1659`

## Problem Statement

We have per-rollout persistence auditing, but no deterministic aggregate proof artifact
covering a full collector run. Live/CI validation cannot publish a stable summary
showing rollout counts, attempts, spans, and gaps in one artifact.

## Scope

In scope:

- add collector-level proof builder aggregating per-rollout audit reports
- include deterministic totals (rollouts by status, attempts, spans, gaps)
- expose machine-readable JSON projection for artifact publication

Out of scope:

- filesystem write paths/CLI commands
- dashboard ingestion
- storage schema changes

## Acceptance Criteria

AC-1 (deterministic aggregate proof):
Given rollout ids,
when proof builder runs,
then it returns deterministic aggregate totals and rollout reports.

AC-2 (retry/requeue integrity proof):
Given retry/requeue rollout execution,
when proof builder runs,
then proof reports all attempts/spans with no gaps.

AC-3 (gap propagation):
Given rollout audits containing persistence gaps,
when proof builder runs,
then proof marks `has_persistence_gaps=true` and carries deterministic gap reasons.

AC-4 (machine-readable artifact):
Given a computed proof,
when JSON projection is requested,
then output is deterministic and machine-readable with schema/version metadata.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given one succeeded rollout id, when proof builds, then totals/statuses/attempts/spans are deterministic and gap-free. |
| C-02 | AC-2 | Integration | Given timeout->success reassignment rollout, when proof builds, then attempt total is 2 and no gaps are reported. |
| C-03 | AC-3 | Conformance | Given hidden attempt record in audit path, when proof builds, then `has_persistence_gaps` is true and reasons include missing attempt record. |
| C-04 | AC-4 | Unit | Given computed proof, when projected to JSON, then schema version and aggregate counters are present with deterministic values. |

## Success Metrics

- collector run integrity can be represented with one deterministic proof object
- proof can be exported as stable JSON for live-run/CI artifacts
- retry/requeue no-data-loss validation remains reproducible
