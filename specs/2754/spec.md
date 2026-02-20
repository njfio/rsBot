# Spec: Issue #2754 - Reconcile G18 decision/stack checklist rows with ADR-006

Status: Implemented

## Problem Statement
The G18 checklist section still marks decision and stack rows as incomplete, despite a completed architecture decision in `docs/architecture/adr-006-dashboard-ui-stack.md`. This mismatch causes roadmap drift and false-negative status reporting.

## Acceptance Criteria

### AC-1 Checklist decision row reflects implemented dashboard decision
Given `tasks/spacebot-comparison.md` G18 section,
When checklist rows are reviewed,
Then the decision row is checked and references the existing ADR-backed decision.

### AC-2 Checklist stack row reflects selected stack
Given `tasks/spacebot-comparison.md` G18 section,
When checklist rows are reviewed,
Then the stack row is checked and references the selected React + TypeScript + Vite direction recorded in ADR-006.

### AC-3 Traceability evidence is explicit
Given the updated checklist rows,
When operators inspect roadmap evidence,
Then issue reference `#2754` is present for auditability.

## Scope

### In Scope
- `tasks/spacebot-comparison.md` G18 decision + stack checklist row updates.
- Milestone/task spec artifacts for traceability.

### Out of Scope
- New architecture decisions or code implementation.
- UI/runtime behavior changes.

## Conformance Cases
- C-01 (docs): G18 decision checklist row is checked and ADR-aligned.
- C-02 (docs): G18 stack checklist row is checked and ADR-aligned.
- C-03 (docs/regression): `#2754` evidence appears in updated G18 checklist rows.

## Success Metrics / Observable Signals
- G18 decision/stack rows are no longer listed as remaining gaps.
- Checklist evidence is consistent with `docs/architecture/adr-006-dashboard-ui-stack.md`.
