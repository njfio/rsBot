# Spec #2170

Status: Implemented
Milestone: specs/milestones/m36/index.md
Issue: https://github.com/njfio/Tau/issues/2170

## Problem Statement

Task #2170 must roll up and verify completion of wave-9 rustdoc coverage work
implemented in subtask #2171, ensuring task-level acceptance evidence is
complete and reproducible.

## Acceptance Criteria

- AC-1: Subtask `#2171` is merged and closed with `status:done`.
- AC-2: Wave-9 guard and scoped quality signals are green on current `master`.
- AC-3: Task closure artifacts (spec/plan/tasks, PR, milestone linkage) are complete.

## Scope

In:

- task-level roll-up artifacts for `#2170`
- verification reruns for wave-9 guard plus scoped checks
- closure label/comment updates for `#2170`

Out:

- additional runtime or behavior changes
- documentation waves outside M36.1.1

## Conformance Cases

- C-01 (AC-1, conformance): `#2171` shows `state=CLOSED`, `status:done`, and merged PR `#2172`.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh` passes on current `master`.
- C-03 (AC-2, functional): `cargo check -p tau-session --target-dir target-fast` and `cargo check -p tau-memory --target-dir target-fast` pass.
- C-04 (AC-3, conformance): task `#2170` is closed with `status:done` and closure comment includes milestone/spec/tests.

## Success Metrics

- `#2170` is closed with full task-level traceability.
- Story `#2169` can close without missing task artifacts.
