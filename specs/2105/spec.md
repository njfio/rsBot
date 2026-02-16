# Spec #2105

Status: Implemented
Milestone: specs/milestones/m28/index.md
Issue: https://github.com/njfio/Tau/issues/2105

## Problem Statement

Task M28.1.1 tracks first-wave rustdoc baseline delivery for split modules with
regression checks. Subtask `#2106` delivered this in PR `#2107`; this task
closes by validating task-level AC/conformance evidence.

## Acceptance Criteria

- AC-1: Scoped first-wave split modules receive baseline rustdoc coverage.
- AC-2: A task-scoped regression guard enforces required doc markers.
- AC-3: Affected crates pass compile/tests with no regressions.

## Scope

In:

- consume merged subtask output from `#2106` / PR `#2107`
- publish task-level lifecycle artifacts and AC mapping
- rerun task-scoped verification commands

Out:

- repository-wide doc-density completion in this task
- behavior changes unrelated to documentation baseline

## Conformance Cases

- C-01 (AC-1, functional): scoped files include expected `///` marker phrases.
- C-02 (AC-2, regression): `bash scripts/dev/test-split-module-rustdoc.sh` passes.
- C-03 (AC-3, functional): compile checks pass for
  `tau-github-issues`, `tau-ai`, and `tau-runtime`.
- C-04 (AC-3, integration): targeted module tests pass in touched crates.

## Success Metrics

- Task issue `#2105` closes with linked subtask evidence.
- `specs/2105/{spec,plan,tasks}.md` lifecycle is complete.
- Story `#2104` roll-up is unblocked.
