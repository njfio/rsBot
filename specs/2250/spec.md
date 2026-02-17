# Spec #2250

Status: Implemented
Milestone: specs/milestones/m46/index.md
Issue: https://github.com/njfio/Tau/issues/2250

## Problem Statement

Task `#2250` tracks closure of M46 critical blockers 1-4 across economics and
training execution paths. Parent closure requires confirmed delivery of child
subtasks `#2254` through `#2258` and synchronized lifecycle metadata.

## Scope

In scope:

- Confirm merged delivery for:
  - `#2254` Gap-1 per-session cost tracking
  - `#2255` Gap-2 token pre-flight estimation
  - `#2256` Gap-3 prompt caching support
  - `#2257` Gap-4 PPO/GAE production wiring
  - `#2258` OpenRouter first-class provider (story-bound dependency)
- Add missing task-level lifecycle artifacts for `#2250`.
- Close `#2250` with `status:done`.

Out of scope:

- New runtime changes beyond completed child subtasks.
- Re-opening accepted child behavior contracts.

## Acceptance Criteria

- AC-1: Given task `#2250`, when validating child issues `#2254..#2258`, then
  each child is closed with implemented artifacts.
- AC-2: Given repository lifecycle policy, when auditing this task, then
  `specs/2250/{spec,plan,tasks}.md` exists and reflects implemented state.
- AC-3: Given issue closure policy, when `#2250` closes, then label is
  `status:done` with closure summary referencing child deliveries.

## Conformance Cases

- C-01 (AC-1, conformance): child issues `#2254`, `#2255`, `#2256`, `#2257`,
  and `#2258` are closed.
- C-02 (AC-2, conformance): `specs/2250/spec.md` status is `Implemented` and
  `specs/2250/tasks.md` status is `Completed`.
- C-03 (AC-3, functional): issue `#2250` closed with `status:done` and closure
  comment referencing merged child outcomes.

## Success Metrics / Observable Signals

- No open blocker subtasks remain under `#2250`.
- Task-level spec lifecycle artifacts are present and finalized.
