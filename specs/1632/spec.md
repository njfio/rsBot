# Issue 1632 Spec

Status: Implemented

Issue: `#1632`  
Milestone: `#21`  
Parent: `#1611`

## Problem Statement

Post-consolidation docs policy requires runbooks to declare ownership surfaces and be validated by docs guardrails. `dashboard-ops` and `custom-command-ops` currently lack explicit ownership sections, and ownership checks do not enforce them.

## Scope

In scope:

- add explicit `## Ownership` sections to dashboard/custom-command runbooks
- update `docs/guides/runbook-ownership-map.md` to include dashboard/custom-command rows
- extend `.github/scripts/runbook_ownership_docs_check.py` to enforce the new ownership tokens
- run docs ownership check + scoped quality checks

Out of scope:

- runtime behavior or CLI semantics changes
- adding dependencies
- unrelated runbook rewrites

## Acceptance Criteria

AC-1 (runbook ownership sections):
Given dashboard/custom-command runbooks,
when reviewed,
then each contains `## Ownership` with crate surfaces and a link to `docs/guides/runbook-ownership-map.md`.

AC-2 (ownership map alignment):
Given `docs/guides/runbook-ownership-map.md`,
when reviewed,
then it includes dashboard/custom-command entries mapped to post-consolidation ownership surfaces.

AC-3 (docs guard enforcement):
Given `.github/scripts/runbook_ownership_docs_check.py`,
when run,
then missing dashboard/custom-command ownership tokens fail closed.

AC-4 (verification):
Given the updated docs and checker,
when scoped checks run,
then ownership check and formatting/lint checks pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given `docs/guides/dashboard-ops.md` and `docs/guides/custom-command-ops.md`, when audited, then `## Ownership` sections + runbook-map links are present. |
| C-02 | AC-2 | Functional | Given `docs/guides/runbook-ownership-map.md`, when audited, then dashboard/custom-command rows are present with ownership surfaces. |
| C-03 | AC-3 | Regression | Given `runbook_ownership_docs_check.py`, when dashboard/custom-command ownership tokens are missing, then check fails; when present, it passes. |
| C-04 | AC-4 | Integration | Given issue-scope commands, when executed, then docs ownership check plus fmt/clippy pass. |

## Success Metrics

- ownership metadata for post-consolidation dashboard/custom-command runbooks is explicit and machine-checked
- docs guardrail prevents future drift for these runbooks
