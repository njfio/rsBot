# Issue 1769 Spec

Status: Accepted

Issue: `#1769`  
Milestone: `#21`  
Parent: `#1760`

## Problem Statement

Roadmap critical-path updates are inconsistent across tracker comments, making
blockers and risk posture hard to compare over time. A standardized template and
risk rubric are needed so recurring updates are structured and machine-checkable.

## Scope

In scope:

- critical-path update template with required status fields and allowed values
- risk scoring rubric (low/med/high + rationale expectations)
- docs references for operator usage in recurring tracker updates
- contract tests to prevent template/rubric drift

Out of scope:

- automated posting bots for tracker comments
- milestone scheduling/cadence enforcement policy (handled by `#1770`)

## Acceptance Criteria

AC-1 (template contract):
Given roadmap operators preparing a critical-path update,
when they use the published template,
then required fields (status, blockers, owner, next action, risk score,
rationale) are present with explicit allowed-value guidance.

AC-2 (risk rubric):
Given a risk score assignment,
when the rubric is referenced,
then low/med/high definitions and rationale requirements are documented in a
machine-readable policy.

AC-3 (tracker publication guidance):
Given tracker update workflows,
when operators consult roadmap docs,
then they can locate the template/rubric paths and apply them consistently.

AC-4 (regression guard):
Given future template/rubric edits,
when contract tests run,
then missing required fields or invalid risk definitions fail deterministically.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given template markdown, when parsed, then required field headings/placeholders are present. |
| C-02 | AC-2 | Functional | Given rubric policy JSON, when loaded, then low/med/high levels and rationale requirements are present. |
| C-03 | AC-3 | Integration | Given roadmap docs index and sync guide, when checked, then template and rubric references are discoverable. |
| C-04 | AC-4 | Regression | Given template/rubric contract tests, when required snippets are removed, then tests fail with deterministic missing-field errors. |

## Success Metrics

- recurring tracker comments can reuse one canonical template path
- risk scoring fields are normalized to low/med/high with rationale text
- contract tests fail closed on template/rubric drift
