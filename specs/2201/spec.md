# Spec #2201

Status: Implemented
Milestone: specs/milestones/m40/index.md
Issue: https://github.com/njfio/Tau/issues/2201

## Problem Statement

Story #2201 must capture and verify completion of M40.1 allow-audit scope
after task #2202 and subtask #2203 merged, ensuring story-level closure
evidence is complete.

## Acceptance Criteria

- AC-1: Task `#2202` is merged/closed with `status:done`.
- AC-2: Story objective is satisfied by implemented allow-audit wave-2 artifacts.
- AC-3: Story closure metadata (spec/plan/tasks, PR, milestone links) is complete.

## Scope

In:

- story-level roll-up artifacts under `specs/2201/`
- verification of closed child task/subtask linkage
- closure label/comment updates for `#2201`

Out:

- new runtime behavior changes
- broader suppression-removal campaigns outside M40.1

## Conformance Cases

- C-01 (AC-1, conformance): `#2202` returns `state=CLOSED` with `status:done`.
- C-02 (AC-2, conformance): `specs/2202/spec.md` and `specs/2203/spec.md` both show `Status: Implemented`.
- C-03 (AC-2, regression): `rg -n "allow\\(" crates -g '*.rs'` reports current retained inventory.
- C-04 (AC-3, conformance): story `#2201` is closed with `status:done` and closure comment references PR/spec/tests.

## Success Metrics

- `#2201` is closed with full story-level traceability.
- Epic `#2200` can close without missing story artifacts.
