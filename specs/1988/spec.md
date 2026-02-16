# Issue 1988 Spec

Status: Accepted

Issue: `#1988`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Summary gate reports can now be exported and replay-validated individually, but
there is no deterministic helper to scan a directory of those reports and build
one audit manifest with pass/fail totals and invalid-file diagnostics.

## Scope

In scope:

- add summary gate report directory manifest models
- add deterministic manifest builder for exported summary gate report directories
- add machine-readable JSON projection for manifest payload

Out of scope:

- dashboard rendering
- remote storage synchronization
- CI workflow wiring

## Acceptance Criteria

AC-1 (deterministic valid manifest):
Given a directory of valid summary gate reports,
when manifest builder runs,
then sorted entries and deterministic pass/fail counters are returned.

AC-2 (invalid file diagnostics):
Given malformed or invalid summary gate report files,
when manifest builder runs,
then invalid-file diagnostics are captured without aborting scan.

AC-3 (machine-readable serialization):
Given a built manifest,
when serialized,
then JSON payload exposes totals plus per-entry sections.

AC-4 (missing directory fail closed):
Given a missing directory path,
when manifest builder runs,
then deterministic missing-directory error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given two valid summary gate reports (one pass, one fail), when manifest builds, then entries are sorted and pass/fail totals match. |
| C-02 | AC-2 | Integration | Given one malformed summary gate report file, when manifest builds, then invalid-file diagnostics contain deterministic parse reason and scan continues. |
| C-03 | AC-3 | Conformance | Given built manifest, when serialized, then payload contains `entries`, `invalid_files`, `pass_reports`, and `fail_reports`. |
| C-04 | AC-4 | Unit | Given missing directory path, when manifest builder runs, then deterministic missing-directory error is returned. |

## Success Metrics

- operators can audit summary gate report batches with one deterministic helper
- malformed files are surfaced as diagnostics instead of aborting scans
- manifest output is machine-readable for automation and downstream checks
