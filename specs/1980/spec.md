# Issue 1980 Spec

Status: Accepted

Issue: `#1980`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Gate report export/validation exists for single files, but there is no
summary-manifest helper for scanning directories of gate reports and producing a
deterministic audit payload with pass/fail totals and invalid-file diagnostics.

## Scope

In scope:

- add gate report directory summary models
- add deterministic summary builder for exported gate report directories
- add machine-readable JSON projection for summary payload

Out of scope:

- dashboard rendering
- remote/object-store transport
- CI workflow wiring

## Acceptance Criteria

AC-1 (deterministic valid summary):
Given a directory of valid gate reports,
when summary builder runs,
then sorted entries and deterministic pass/fail counters are returned.

AC-2 (invalid file diagnostics):
Given malformed or invalid gate report files in the directory,
when summary builder runs,
then invalid-file diagnostics are captured without aborting scan.

AC-3 (machine-readable serialization):
Given a built summary,
when serialized,
then JSON payload exposes totals plus per-entry sections.

AC-4 (missing directory fail closed):
Given a missing directory path,
when summary builder runs,
then deterministic missing-directory error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given two valid gate reports (one pass, one fail), when summary builds, then entries are sorted and pass/fail totals match. |
| C-02 | AC-2 | Integration | Given one malformed gate report file, when summary builds, then invalid-file diagnostics contain deterministic parse reason and scan continues. |
| C-03 | AC-3 | Conformance | Given built summary, when serialized, then payload contains `entries`, `invalid_files`, `pass_entries`, and `fail_entries`. |
| C-04 | AC-4 | Unit | Given missing directory path, when summary builder runs, then deterministic missing-directory error is returned. |

## Success Metrics

- operators can audit gate report batches with a single deterministic helper
- malformed files are surfaced as diagnostics instead of aborting scans
- summary output is machine-readable for automation and downstream checks
