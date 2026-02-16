---
name: Subtask
about: Small executable unit under one task.
title: "Subtask: "
labels:
  - "type:subtask"
  - "process:spec-driven"
  - "process:tdd"
  - "priority:P1"
  - "status:todo"
  - "testing-matrix"
---

## Parent

Parent: `#<task-id>`

Subtasks require exactly one parent task.

## Work Item

Describe the narrow implementation/test slice for this subtask.

## Milestone

Set exactly one milestone linked to `specs/milestones/<id>/index.md`.

## Dependencies

List predecessor subtasks/tasks that must land first.

## Risk

Set risk level (`low|med|high`) with rationale.

## Required Labels

Ensure labels include all namespaces:

- `type:` (`type:subtask`)
- `area:` (choose one)
- `process:` (`process:spec-driven`, `process:tdd`)
- `priority:` (`priority:P0|P1|P2`)
- `status:` (`status:todo` initially)

## Definition of Ready

- [ ] Parent task is linked.
- [ ] Milestone is set.
- [ ] Dependencies are linked.
- [ ] Risk is set with rationale.
- [ ] Spec/plan/tasks mapping for parent issue is available.
- [ ] Test expectation (red/green evidence) is identified.
