# Issue 1626 Spec

Status: Implemented

Issue: `#1626`  
Milestone: `#21`  
Parent: `#1610`

## Problem Statement

Scaffold consolidation execution lacks a reproducible inventory of candidate
surfaces with ownership and objective runtime/test touchpoint signals.
Without a deterministic inventory artifact, follow-on merge/remove execution
work remains ambiguous.

## Scope

In scope:

- define inventory schema for scaffold candidate reporting
- implement deterministic inventory scanner script
- emit machine-readable JSON + markdown ownership-map report snapshot
- include clear regeneration/update instructions in the report

Out of scope:

- applying merge/remove code changes for candidates
- CI workflow wiring changes
- dependency additions

## Acceptance Criteria

AC-1 (inventory schema):
Given scaffold candidate inventory requirements,
when schema is published,
then ownership, source-size, runtime-reference, and test-touchpoint fields are
defined for each candidate.

AC-2 (deterministic scanner/report):
Given fixed generated timestamp and candidate input,
when scanner runs,
then deterministic JSON and markdown artifacts are produced.

AC-3 (ownership completeness):
Given generated inventory,
when ownership map is read,
then every candidate surface has a non-empty owner value.

AC-4 (fail-closed regression):
Given invalid candidate metadata (for example blank owner),
when scanner runs,
then deterministic validation error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given schema file, when loaded, then required ownership/runtime/test fields are present. |
| C-02 | AC-2 | Conformance | Given fixed timestamp, when scanner runs repeatedly, then JSON/MD hashes are stable. |
| C-03 | AC-3 | Functional | Given generated inventory, when validated, then missing-owner count is zero and each candidate has owner text. |
| C-04 | AC-4 | Regression | Given fixture candidate with blank owner, when scanner runs, then fail-closed owner validation error is returned. |

## Success Metrics

- scaffold inventory is regenerable with one script command
- ownership column is complete and machine-readable
- invalid metadata is blocked before downstream execution planning
