---
name: Story
about: Feature slice under one epic.
title: "Story: "
labels:
  - "type:story"
  - "process:spec-driven"
  - "process:tdd"
  - "priority:P1"
  - "status:todo"
---

## Parent

Parent: `#<epic-id>`

Stories require exactly one parent epic.

## Goal

Describe the user-visible capability delivered by this story.

## Milestone

Set exactly one milestone linked to `specs/milestones/<id>/index.md`.

## Dependencies

List prerequisite stories/tasks and blocking risks.

## Risk

Set risk level (`low|med|high`) with rationale.

## Required Labels

Ensure labels include all namespaces:

- `type:` (`type:story`)
- `area:` (choose one)
- `process:` (`process:spec-driven`, `process:tdd`)
- `priority:` (`priority:P0|P1|P2`)
- `status:` (`status:todo` initially)

## Definition of Ready

- [ ] Parent epic is linked.
- [ ] Milestone is set.
- [ ] Dependencies are linked.
- [ ] Risk is set with rationale.
- [ ] Story acceptance criteria are testable.
- [ ] Spec path is planned (`specs/<issue-id>/spec.md`).
