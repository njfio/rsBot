# Issue 1972 Spec

Status: Implemented

Issue: `#1972`  
Milestone: `#24`  
Parent: `#1662`

## Problem Statement

Benchmark artifacts are exported as individual JSON files, but there is no
deterministic directory manifest that summarizes valid and invalid artifacts for
audit/replay workflows.

## Scope

In scope:

- add benchmark artifact directory manifest builder in `tau-trainer`
- scan `.json` artifact files with deterministic ordering
- include valid manifest entries plus invalid-file diagnostics
- provide machine-readable manifest JSON serialization

Out of scope:

- remote/object store scanning
- dashboard rendering/integration
- async file watchers

## Acceptance Criteria

AC-1 (deterministic sorted entries):
Given a directory with exported artifact files,
when manifest builder runs,
then resulting entries are deterministic and sorted by file path.

AC-2 (mixed validity handling):
Given a directory with valid and invalid artifact files,
when manifest builder runs,
then valid entries are preserved and invalid files are reported with reasons
without aborting the scan.

AC-3 (machine-readable manifest):
Given a built manifest,
when serialized,
then payload includes scan totals, valid entries, and invalid-file diagnostics.

AC-4 (missing directory error):
Given a missing directory path,
when manifest builder runs,
then deterministic missing-directory error is returned.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given two exported artifacts in unsorted filesystem order, when manifest is built, then entries are sorted deterministically by path. |
| C-02 | AC-2 | Integration | Given one valid and one malformed artifact file, when manifest is built, then one valid entry and one invalid diagnostic are returned. |
| C-03 | AC-3 | Conformance | Given a non-empty manifest, when `to_json_value` runs, then totals/entries/invalid sections are machine-readable JSON fields. |
| C-04 | AC-4 | Unit | Given a missing directory path, when manifest builder runs, then deterministic error mentions missing directory. |

## Success Metrics

- one helper call produces deterministic artifact-directory summary
- invalid artifacts are surfaced with explicit diagnostics instead of hard scan failures
- manifest payload is ready for archival and downstream tooling
