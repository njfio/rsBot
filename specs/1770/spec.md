# Issue 1770 Spec

Status: Accepted

Issue: `#1770`  
Milestone: `#21`  
Parent: `#1760`

## Problem Statement

`#1769` added a critical-path update template and risk rubric, but update timing is
not enforced. Missing or stale updates reduce confidence in milestone execution
status and delay blocker escalation.

## Scope

In scope:

- cadence policy for critical-path update frequency and grace window
- escalation policy for missed updates
- checklist artifact for tracker update operators
- contract tests for cadence policy/checklist/script/docs coherence

Out of scope:

- fully automated reminder bot integrations outside local scripts
- cross-repo scheduling orchestration

## Acceptance Criteria

AC-1 (cadence policy):
Given critical-path tracker updates,
when cadence policy is evaluated,
then expected update frequency and grace window are explicit in a versioned
policy file.

AC-2 (escalation path):
Given stale or missing updates,
when cadence checks run,
then escalation thresholds and required actions are deterministic and visible.

AC-3 (tracker checklist):
Given operators preparing recurring updates,
when they use the checklist,
then required pre-publish and escalation acknowledgment steps are present.

AC-4 (enforcement tooling):
Given tracker comments (live or fixture),
when cadence-check tooling runs,
then it reports pass/fail based on policy thresholds.

AC-5 (documentation):
Given roadmap status docs,
when operators consult the runbook,
then cadence policy, checklist, and cadence-check commands are discoverable.

## Conformance Cases

| Case | Maps To | Tier | Given / When / Then |
| --- | --- | --- | --- |
| C-01 | AC-1 | Functional | Given cadence policy JSON, when loaded, then cadence/grace and tracker issue fields are valid. |
| C-02 | AC-2 | Functional | Given stale fixture comments, when cadence check runs, then escalation status and reason are returned. |
| C-03 | AC-3 | Functional | Given checklist markdown, when parsed, then required checklist items and escalation acknowledgment fields are present. |
| C-04 | AC-4 | Regression | Given in-window fixture comments, when cadence check runs, then status passes; missing-update fixtures fail deterministically. |
| C-05 | AC-5 | Integration | Given roadmap docs and docs index, when contract tests run, then cadence assets/commands are referenced. |

## Success Metrics

- cadence policy and escalation path are versioned and test-validated
- operators can execute a single cadence-check command pre-update
- missed/stale updates produce deterministic non-zero enforcement output
