# Spec #2087

Status: Implemented
Milestone: specs/milestones/m26/index.md
Issue: https://github.com/njfio/Tau/issues/2087

## Problem Statement

Story M26.1 enforces oversized-exemption contract integrity so stale exemptions
cannot persist silently. Task `#2088` delivered task-level closure on merged
subtask implementation. This story closes by validating task completion and
story-level governance outcomes.

## Acceptance Criteria

- AC-1: Story child task(s) complete with linked evidence.
- AC-2: Oversized exemption metadata accuracy is enforced via fail-closed
  policy checks.
- AC-3: Guardrail and contract suites remain green after stale-exemption
  cleanup.

## Scope

In:

- consume completed task `#2088`
- map story ACs to conformance/test evidence
- publish story closure artifacts and status updates

Out:

- additional policy domains beyond oversized exemption enforcement

## Conformance Cases

- C-01 (AC-1, integration): child task `#2088` is closed with `status:done`.
- C-02 (AC-2, functional): stale-exemption policy regression suite passes.
- C-03 (AC-3, regression): guardrail contract + Python oversized guard suites
  pass.
- C-04 (AC-1..AC-3, integration): direct oversized guard command reports
  `issues=0`.

## Success Metrics

- Story `#2087` closes with linked PR/test evidence.
- Story artifacts `specs/2087/{spec,plan,tasks}.md` are implemented.
- Parent epic `#2086` can consume closure evidence.
