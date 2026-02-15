# Issue 1703 Spec

Status: Accepted

Issue: `#1703`  
Milestone: `#21`  
Parent: `#1699`

## Problem Statement

The M21 validation matrix artifact exists but is stale and not fully suitable as a
closure gate artifact. It must be regenerated from current milestone state and
present reproducible artifact references suitable for issue comments and gate
review.

## Scope

In scope:

- refresh `tasks/reports/m21-validation-matrix.{json,md}` from current M21 data
- ensure matrix local artifact paths are repository-relative (portable links)
- publish matrix evidence in `#1703` gate issue

Out of scope:

- changing milestone scope or acceptance criteria definitions
- closing unrelated structural execution issues

## Acceptance Criteria

AC-1 (fresh matrix generation):
Given current M21 issue state,
when matrix generation runs,
then `m21-validation-matrix.json` and `.md` are regenerated with current
summary counts and issue rows.

AC-2 (portable artifact links):
Given matrix local artifacts,
when matrix output is generated,
then artifact paths are repository-relative instead of environment-specific
absolute paths.

AC-3 (gate evidence publication):
Given refreshed matrix outputs,
when issue `#1703` is updated,
then gate comment includes matrix summary and artifact references.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given live/fixture issue state, when matrix script runs, then JSON/MD outputs exist with non-empty summary and issue matrix sections. |
| C-02 | AC-2 | Regression | Given fixture report artifacts, when matrix script runs, then local artifact paths do not include absolute prefixes and are repo-relative. |
| C-03 | AC-3 | Integration | Given refreshed outputs, when gate issue comment is posted, then summary metrics and artifact paths are present and reproducible. |

## Success Metrics

- matrix regenerated with current milestone counts and issue coverage
- no absolute-worktree path references in local artifact table
- gate issue comment contains auditable summary + artifact locations
