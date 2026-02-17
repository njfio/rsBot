# Spec #2252

Status: Implemented
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2252

## Problem Statement

Task `#2252` tracks closure of M46 distribution gaps 10-13. Parent closure
requires confirmation that delivery subtasks `#2263..#2266` are complete and
task lifecycle artifacts are synchronized.

## Scope

In scope:

- Confirm merged delivery for:
  - `#2263` Docker image packaging
  - `#2264` Homebrew formula
  - `#2265` shell completions
  - `#2266` systemd unit
- Add missing task-level lifecycle docs for `#2252`.
- Close `#2252` with `status:done`.

Out of scope:

- New packaging behavior beyond merged child implementations.

## Acceptance Criteria

- AC-1: Given task `#2252`, when validating child issues `#2263..#2266`, then
  all are closed with merged deliverables.
- AC-2: Given lifecycle policy, when auditing the task, then
  `specs/2252/{spec,plan,tasks}.md` exists and indicates completion.
- AC-3: Given closure policy, when `#2252` closes, then `status:done` is set and
  closure summary references completed child issues.

## Conformance Cases

- C-01 (AC-1, conformance): child issues `#2263`, `#2264`, `#2265`, and
  `#2266` are closed.
- C-02 (AC-2, conformance): `specs/2252/spec.md` status is `Implemented` and
  `specs/2252/tasks.md` status is `Completed`.
- C-03 (AC-3, functional): issue `#2252` is closed with `status:done` and
  completion comment.

## Success Metrics / Observable Signals

- No open subtask remains under distribution task `#2252`.
- Task lifecycle artifacts exist and are finalized.
