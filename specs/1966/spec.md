# Issue 1966 Spec

Status: Implemented

Issue: `#1966`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

`tau-trainer` computes benchmark significance and promotion gate decisions, but
there is no single deterministic artifact payload that bundles these outputs for
archival, replay, and downstream validation.

## Scope

In scope:

- add a typed benchmark evaluation artifact in `tau-trainer`
- include policy improvement report, optional reproducibility sections, and
  checkpoint promotion decision
- add deterministic JSON serialization with explicit schema version metadata

Out of scope:

- external persistence/transport
- dashboard rendering
- statistical-method changes

## Acceptance Criteria

AC-1 (deterministic typed bundle):
Given valid benchmark significance and promotion-gate outputs,
when artifact builder runs,
then it returns a deterministic typed artifact with all required sections.

AC-2 (machine-readable schema payload):
Given a built artifact,
when converted to JSON,
then payload includes schema metadata and machine-readable sections.

AC-3 (reason-code preservation):
Given a blocked checkpoint promotion decision,
when bundled into artifact,
then reason codes are preserved without mutation.

AC-4 (optional section nullability):
Given missing reproducibility sections,
when serialized,
then JSON remains valid and those sections are explicitly `null`.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given valid policy/significance inputs, when builder runs, then typed artifact fields are deterministic and complete. |
| C-02 | AC-2 | Conformance | Given artifact output, when serialized, then schema/version fields and benchmark section payloads are present as JSON objects. |
| C-03 | AC-3 | Integration | Given blocked promotion decision with reason codes, when artifact is built, then output preserves all reason codes and order. |
| C-04 | AC-4 | Unit | Given `None` for reproducibility reports, when artifact serializes, then corresponding JSON fields are `null`. |

## Success Metrics

- one stable artifact payload to represent benchmark-evaluation outcomes
- no data loss between typed reports and serialized artifact
- deterministic schema version marker for compatibility checks
