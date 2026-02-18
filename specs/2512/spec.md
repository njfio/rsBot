# Spec #2512 - Epic: G12 skip-tool closure + validation

Status: Accepted

## Problem Statement
G12 is still marked incomplete in `tasks/spacebot-comparison.md` even though skip behavior exists across multiple crates. We need formal closure with spec-driven evidence.

## Acceptance Criteria
### AC-1
Given the G12 pathway, when validation is complete, then all checklist items are backed by passing conformance evidence.

### AC-2
Given implementation and validation completion, when closure runs, then linked story/task/subtask are merged and marked done.

## Scope
In scope:
- G12 validation chain and evidence collation.
- Checklist update.

Out of scope:
- New process architecture or non-G12 gap items.

## Conformance Cases
- C-01 (AC-1): Task-level conformance matrix in #2514 passes.
- C-02 (AC-2): PR merged, linked issues closed with status `done`.

## Success Metrics
- G12 checkboxes are all `[x]`.
- Issue chain #2512-#2515 closed with specs marked Implemented.
