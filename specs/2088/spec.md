# Spec #2088

Status: Implemented
Milestone: specs/milestones/m26/index.md
Issue: https://github.com/njfio/Tau/issues/2088

## Problem Statement

Task M26.1.1 requires repository-level delivery of stale-exemption cleanup and
active-size eligibility enforcement for oversized-file policy metadata.
Subtask `#2089` implemented these changes; this task closes by validating those
deliverables against task acceptance criteria.

## Acceptance Criteria

- AC-1: Stale oversized-file exemptions are removed from repository policy
  metadata.
- AC-2: Policy validation rejects exemptions unless the referenced file is
  currently above default threshold.
- AC-3: Shell/Python guardrail suites remain green with stale regression
  coverage.

## Scope

In:

- consume merged subtask implementation from `#2089` / PR `#2090`
- map task ACs to concrete conformance cases and tests
- publish task-level closure evidence and lifecycle artifacts

Out:

- additional decomposition work unrelated to stale-exemption contract
- threshold changes beyond current policy defaults

## Conformance Cases

- C-01 (AC-1, integration): `tasks/policies/oversized-file-exemptions.json`
  contains no stale exemption entries.
- C-02 (AC-2, functional): stale-exemption regression in
  `scripts/dev/test-oversized-file-policy.sh` fails closed.
- C-03 (AC-3, regression): guardrail contract and oversized-file Python tests
  pass after cleanup.
- C-04 (AC-1..AC-3, integration): direct oversized guard run returns `issues=0`
  with repository policy paths.

## Success Metrics

- Task issue `#2088` closes with conformance evidence linked to PR `#2090`.
- All mapped suites pass on latest `master`.
- `specs/2088/{spec,plan,tasks}.md` lifecycle is completed.
