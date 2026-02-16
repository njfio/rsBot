# Issue 1992 Spec

Status: Accepted

Issue: `#1992`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Summary gate report manifests and manifest-quality decisions exist as separate
helpers, but there is no single deterministic combined report payload for
automation/audit consumers.

## Scope

In scope:

- add combined summary gate manifest report model (manifest + quality)
- add builder helper wiring manifest-quality evaluation into report output
- add machine-readable JSON projection for combined report payload

Out of scope:

- CI workflow wiring
- dashboard rendering
- remote transport/storage

## Acceptance Criteria

AC-1 (deterministic combined report):
Given a summary gate manifest and quality policy,
when report builder runs,
then deterministic manifest counters and quality decision fields are preserved.

AC-2 (reason propagation):
Given a failing manifest-quality outcome,
when report is built,
then manifest-quality reason codes propagate in output.

AC-3 (machine-readable serialization):
Given a combined manifest report,
when serialized,
then JSON payload exposes nested `manifest` and `quality` objects.

AC-4 (invalid policy fail closed):
Given invalid manifest-quality policy ratios (`>1` or `<0`),
when report builder runs,
then deterministic validation error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given manifest(pass=2, fail=0, invalid=0), when report builds, then quality pass=true and manifest counters match input. |
| C-02 | AC-2 | Integration | Given manifest(pass=0, fail=2, invalid=1), when report builds, then quality reason codes include threshold-failure reasons. |
| C-03 | AC-3 | Conformance | Given built report, when serialized, then payload contains nested `manifest` and `quality` objects with machine-readable fields. |
| C-04 | AC-4 | Unit | Given policy with `max_fail_ratio=1.5`, when report builds, then deterministic out-of-range error is returned. |

## Success Metrics

- one helper returns full manifest+quality report for operators
- no data loss between manifest counters and quality decision output
- JSON output is directly consumable by automation without custom parsing
