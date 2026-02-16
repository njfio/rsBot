---
name: Task
about: Implementation task under one story.
title: "Task: "
labels:
  - "type:task"
  - "process:spec-driven"
  - "process:tdd"
  - "priority:P1"
  - "status:todo"
  - "testing-matrix"
---

## Parent

Parent: `#<story-id>`

Tasks require exactly one parent story.

## Problem / Delivery Slice

Describe the specific implementation slice this task delivers.

## Milestone

Set exactly one milestone linked to `specs/milestones/<id>/index.md`.

## Dependencies

List blocking tasks, services, or PR prerequisites.

## Risk

Set risk level (`low|med|high`) with rationale.

## Required Labels

Ensure labels include all namespaces:

- `type:` (`type:task`)
- `area:` (choose one)
- `process:` (`process:spec-driven`, `process:tdd`)
- `priority:` (`priority:P0|P1|P2`)
- `status:` (`status:todo` initially)

## Definition of Ready

- [ ] Parent story is linked.
- [ ] Milestone is set.
- [ ] Dependencies are linked.
- [ ] Risk is set with rationale.
- [ ] `specs/<issue-id>/spec.md` exists and is accepted before implementation.
- [ ] Acceptance criteria map to conformance cases and tests.
