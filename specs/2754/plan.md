# Plan: Issue #2754 - Reconcile G18 decision/stack checklist rows with ADR-006

## Approach
1. Confirm ADR-006 still defines dashboard decision + selected stack.
2. Update the two unchecked G18 checklist rows to checked state with explicit `#2754` evidence.
3. Verify no other checklist semantics changed.

## Affected Modules
- `tasks/spacebot-comparison.md`
- `specs/milestones/m122/index.md`
- `specs/2754/spec.md`
- `specs/2754/tasks.md`

## Risks / Mitigations
- Risk: checklist wording drifts from ADR language.
  - Mitigation: mirror ADR terminology directly.

## Interfaces / Contracts
- Documentation-only change; no API/runtime contracts touched.

## ADR
- Reuses existing `docs/architecture/adr-006-dashboard-ui-stack.md`; no new ADR required.
