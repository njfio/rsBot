# Issue 1974 Spec

Status: Implemented

Issue: `#1974`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Manifest scanning returns valid/invalid artifact counts, but there is no
deterministic quality gate to decide whether a scan result is acceptable for
downstream benchmark replay and promotion workflows.

## Scope

In scope:

- add manifest quality policy + decision types
- evaluate manifest counts/ratios against deterministic thresholds
- emit deterministic reason codes and machine-readable decision payload

Out of scope:

- CI workflow wiring
- dashboard/visualization surfaces
- auto-remediation actions

## Acceptance Criteria

AC-1 (pass path):
Given a manifest that meets policy thresholds,
when quality gate runs,
then decision passes with empty reason codes.

AC-2 (no valid artifacts fail path):
Given a manifest with zero valid entries,
when quality gate runs,
then decision fails with reason code `no_valid_artifacts`.

AC-3 (invalid ratio fail path):
Given a manifest whose invalid ratio exceeds policy maximum,
when quality gate runs,
then decision fails with reason code `invalid_ratio_exceeded`.

AC-4 (machine-readable decision payload):
Given a quality decision,
when serialized,
then payload includes policy counters, computed ratio, pass/fail, and reasons.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given manifest with valid=3 invalid=0 and permissive policy, when quality gate runs, then pass=true and reasons empty. |
| C-02 | AC-2 | Unit | Given manifest with valid=0 invalid>0, when quality gate runs, then fail with `no_valid_artifacts`. |
| C-03 | AC-3 | Integration | Given manifest with invalid ratio above threshold, when quality gate runs, then fail with `invalid_ratio_exceeded`. |
| C-04 | AC-4 | Conformance | Given a decision, when serialized, then machine-readable fields include counts, ratio, thresholds, and reason codes. |

## Success Metrics

- deterministic gate decision from manifest scans in one helper call
- explicit reason codes for operator/audit triage
- machine-readable policy decision payload for downstream automation
