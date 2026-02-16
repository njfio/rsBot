---
name: Epic
about: Large cross-cutting initiative tracked under one milestone.
title: "Epic: "
labels:
  - "type:epic"
  - "process:spec-driven"
  - "process:tdd"
  - "priority:P1"
  - "status:todo"
---

## Objective

Describe the end-state outcome for this epic.

## Milestone

Set exactly one milestone. The milestone description must link
`specs/milestones/<id>/index.md`.

## Dependencies

List blocking epics/stories/tasks and ordering constraints.

## Risk

Set risk level (`low|med|high`) with one-sentence rationale.

## Required Labels

Ensure labels include each required namespace:

- `type:` (this template uses `type:epic`)
- `area:` (choose one: backend/frontend/networking/qa/devops/docs/governance)
- `process:` (`process:spec-driven`, `process:tdd`)
- `priority:` (`priority:P0|P1|P2`)
- `status:` (`status:todo` initially)

## Definition of Ready

- [ ] Milestone is set and linked to a milestone spec container.
- [ ] Scope and acceptance criteria are testable.
- [ ] Dependencies are linked.
- [ ] Risk level is set with rationale.
- [ ] Spec artifact path is declared (`specs/<issue-id>/spec.md`).
