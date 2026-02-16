# Issue 1976 Spec

Status: Accepted

Issue: `#1976`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Manifest scanning and quality gating exist as separate helpers, but there is no
single deterministic report payload that bundles both outputs for audit and
automation consumers.

## Scope

In scope:

- add gate report model that embeds manifest summary and quality decision
- add builder helper that runs quality evaluation from manifest counters
- add machine-readable JSON projection for gate reports

Out of scope:

- CI workflow enforcement
- dashboard rendering
- remote storage transport

## Acceptance Criteria

AC-1 (deterministic combined report):
Given a manifest and quality policy,
when report builder runs,
then report includes deterministic manifest counters and quality decision fields.

AC-2 (reason propagation):
Given a failing manifest/policy outcome,
when report is built,
then quality reason codes are preserved in report output.

AC-3 (machine-readable serialization):
Given a gate report,
when serialized,
then JSON payload exposes nested manifest and quality sections.

AC-4 (invalid policy fail closed):
Given an invalid quality policy (`max_invalid_ratio` outside `[0,1]`),
when report builder runs,
then deterministic validation error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given manifest(valid=2, invalid=0) and permissive policy, when report builds, then pass=true and counts match manifest. |
| C-02 | AC-2 | Integration | Given manifest(valid=0, invalid=2), when report builds, then report quality reasons include `no_valid_artifacts`. |
| C-03 | AC-3 | Conformance | Given built report, when serialized, then payload contains `manifest` and `quality` objects with machine-readable fields. |
| C-04 | AC-4 | Unit | Given policy with `max_invalid_ratio=1.5`, when report builds, then deterministic out-of-range error is returned. |

## Success Metrics

- one helper returns a complete manifest+quality report for operators
- no data loss between manifest counters and quality decision output
- JSON output is directly consumable by automation without manual parsing logic
