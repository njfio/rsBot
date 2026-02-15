# Issue 1704 Spec

Status: Accepted

Issue: `#1704`  
Milestone: `#21`  
Parent: `#1699`

## Problem Statement

M21 requires a reproducible live proof pack showing retained runtime capabilities
are executable with current code. Proof artifacts are not yet generated/attached
for the gate issue, so closure evidence is incomplete.

## Scope

In scope:

- execute retained-capability live proof matrix
- emit proof summary artifacts under `tasks/reports/`
- ensure proof artifact references are portable for issue linkage
- publish proof summary + troubleshooting notes in issue `#1704`

Out of scope:

- closing unrelated structural feature tasks
- redefining retained-capability matrix scope outside current script contract

## Acceptance Criteria

AC-1 (live proof execution):
Given the retained-capability proof matrix,
when the proof summary script runs,
then run results execute and output JSON/Markdown summaries with pass/fail status.

AC-2 (artifact portability):
Given proof summary outputs,
when artifact paths are emitted,
then path references are repository-relative (portable) for tracker linkage.

AC-3 (proof pack publication):
Given generated proof artifacts,
when issue `#1704` is updated,
then summary metrics, artifact references, and troubleshooting notes are posted.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given script test matrix, when proof summary script runs, then it emits JSON/MD outputs and pass/fail run summaries. |
| C-02 | AC-2 | Regression | Given proof summary output, when paths are inspected, then artifact/log/report paths are relative rather than absolute. |
| C-03 | AC-3 | Integration | Given refreshed proof artifacts, when #1704 is updated, then gate comment includes evidence table and troubleshooting section. |

## Success Metrics

- proof summary artifacts exist and are reproducible from scripted command
- artifact paths in proof summary are portable for markdown/issue linkage
- issue #1704 contains complete proof evidence and troubleshooting notes
