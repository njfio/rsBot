# Issue 1699 Spec

Status: Implemented

Issue: `#1699`  
Milestone: `#21`  
Parent Epics: `#1605`, `#1606`, `#1607`

## Problem Statement

Milestone `#21` requires a deterministic exit gate proving structural hardening
outcomes are complete and evidenced by current artifacts. The gate story must
aggregate milestone state and verify core guardrails (oversized-file and safety
live-proof contracts) before closure.

## Scope

In scope:

- refresh and publish current M21 validation and split-validation artifacts
- verify oversized-file and safety live-run contract checks remain green
- document exit-gate evidence and close remaining milestone containers

Out of scope:

- new runtime behavior changes
- non-M21 milestone work
- dependency or protocol changes

## Acceptance Criteria

AC-1 (milestone closure posture):
Given milestone `#21`,
when evaluating open issues for stories/tasks/epics,
then all required execution containers are closed prior to gate closure.

AC-2 (oversized-file guardrail evidence):
Given M21 structural split work,
when running oversized-file guardrail contract checks,
then guardrail validation passes and evidence remains linked in current reports.

AC-3 (safety live-proof evidence):
Given safety mainline merge deliverables,
when running safety live-run validation contract checks,
then safety proof validation passes and retained-proof artifacts remain present.

AC-4 (consolidated validation artifacts):
Given milestone state and local reports,
when regenerating M21 matrix artifacts,
then `m21-validation-matrix` and `m21-tool-split-validation` artifacts are
updated and referenced in closure evidence.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given milestone `#21` issue state, when queried, then only the gate issue remains open before final close. |
| C-02 | AC-2 | Regression | Given guardrail policy scripts, when running oversized-file guardrail contract tests, then pass status is returned. |
| C-03 | AC-3 | Regression | Given safety validation scripts, when running safety live-run contract tests, then pass status is returned. |
| C-04 | AC-4 | Conformance | Given M21 report generation scripts, when executed, then `tasks/reports/m21-validation-matrix.{json,md}` and `tasks/reports/m21-tool-split-validation.{json,md}` are regenerated successfully. |

## Success Metrics

- milestone `#21` closure gate issue contains complete evidence summary
- refreshed M21 report artifacts are committed and reproducible
- M21 containers (`#1610/#1611/#1612` + `#1605/#1606/#1607`) are closed
