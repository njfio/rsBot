# Issue 1639 Spec

Status: Implemented

Issue: `#1639`  
Milestone: `#21`  
Parent: `#1615`

## Problem Statement

Oversized-file guardrail code, tests, and CI wiring exist, but parent task
`#1639` remains open without issue-specific spec artifacts and parent-level
conformance evidence connecting CI enforcement, exemption auditability, and
operator remediation docs.

## Scope

In scope:

- add `specs/1639/{spec,plan,tasks}.md`
- add issue harness validating oversized-file guardrail contract markers across
  guard script, tests, CI workflow, exemption policy, and docs
- document CI guardrail workflow contract details in policy guide
- run targeted guardrail tests and scoped quality checks

Out of scope:

- changing guard thresholds or exemption entries
- adding new CI jobs beyond current wiring
- unrelated structural split/refactor implementation

## Acceptance Criteria

AC-1 (CI guardrail enforcement):
Given CI workflow and guard script,
when checked,
then oversized production files are blocked with actionable annotation/report
paths.

AC-2 (exemption auditability):
Given exemption policy JSON and validator tests,
when checked,
then exemption metadata contract remains explicit and auditable.

AC-3 (documentation/remediation clarity):
Given oversized-file policy guide,
when reviewed,
then CI guardrail workflow and remediation links are explicit.

AC-4 (verification):
Given issue-scope checks,
when run,
then harness + targeted tests + roadmap/fmt/clippy checks pass.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | CI verification | Given CI workflow + guard script, when checked by harness, then threshold check and artifact upload wiring are present. |
| C-02 | AC-2 | Functional | Given guard and policy tests, when run, then exemption metadata and oversized checks remain deterministic. |
| C-03 | AC-3 | Functional | Given policy guide, when checked by harness, then guardrail workflow contract and remediation pointers are present. |
| C-04 | AC-4 | Integration | Given issue commands, when run, then harness + targeted tests + roadmap/fmt/clippy pass. |

## Success Metrics

- `#1639` has complete spec-driven closure artifacts
- oversized-file CI guardrail contract is explicit and test-backed
- exemption/remediation policy remains auditable for reviewers/operators
