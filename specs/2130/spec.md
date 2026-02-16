# Spec #2130

Status: Implemented
Milestone: specs/milestones/m31/index.md
Issue: https://github.com/njfio/Tau/issues/2130

## Problem Statement

Task #2130 must roll up and verify completion of wave-4 rustdoc coverage work
executed in subtask #2131, ensuring acceptance criteria and validation evidence
are fully recorded at the task level.

## Acceptance Criteria

- AC-1: Subtask `#2131` is merged and closed with `status:done`.
- AC-2: Wave-4 guard and scoped quality signals remain green on current `master`.
- AC-3: Task closure artifact links (`spec/plan/tasks`, PR, milestone) are complete.

## Scope

In:

- task-level roll-up specification for `#2130`
- verification rerun on `master` for guard + scoped quality commands
- closure comments/labels for `#2130`

Out:

- new runtime or behavior changes
- additional module documentation waves outside M31.1.1

## Conformance Cases

- C-01 (AC-1, conformance): `gh issue view 2131` shows `state=CLOSED` and includes merged PR `#2132`.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh` passes on current `master`.
- C-03 (AC-2, functional): `cargo check -p tau-runtime --target-dir target-fast` and `cargo check -p tau-github-issues --target-dir target-fast` pass.
- C-04 (AC-3, conformance): task `#2130` is closed with `status:done` and closure comment linking milestone/spec/tests.

## Success Metrics

- `#2130` is closed with complete audit trail and no open ACs.
- M31 story/epic roll-up can proceed without missing task artifacts.
