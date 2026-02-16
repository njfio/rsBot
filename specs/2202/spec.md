# Spec #2202

Status: Implemented
Milestone: specs/milestones/m40/index.md
Issue: https://github.com/njfio/Tau/issues/2202

## Problem Statement

Task #2202 must roll up and verify completion of allow-pragmas audit wave-2
work implemented in subtask #2203, ensuring task-level evidence is complete and
reproducible.

## Acceptance Criteria

- AC-1: Subtask `#2203` is merged and closed with `status:done`.
- AC-2: Allow-audit wave-2 signals are green on current `master`.
- AC-3: Task closure artifacts (spec/plan/tasks, PR, milestone linkage) are complete.

## Scope

In:

- task-level roll-up artifacts for `#2202`
- verification reruns for allow inventory and scoped `tau-algorithm` checks
- closure label/comment updates for `#2202`

Out:

- additional runtime/algorithm changes beyond merged wave-2 cleanup
- broader repository lint campaigns

## Conformance Cases

- C-01 (AC-1, conformance): `#2203` shows `state=CLOSED`, `status:done`, and merged PR `#2204`.
- C-02 (AC-2, regression): `rg -n "allow\\(" crates -g '*.rs'` reports current inventory and omits removed stale suppression in `ppo.rs`.
- C-03 (AC-2, functional): `cargo check -p tau-algorithm --target-dir target-fast` passes.
- C-04 (AC-2, integration): `cargo test -p tau-algorithm ppo --target-dir target-fast` passes.
- C-05 (AC-3, conformance): task `#2202` is closed with `status:done` and closure comment includes milestone/spec/tests.

## Success Metrics

- `#2202` is closed with full task-level traceability.
- Story `#2201` can close without missing task artifacts.
