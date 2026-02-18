# M87 - G11 Message Coalescing Closure

Status: InProgress

## Goal
Close G11 from `tasks/spacebot-comparison.md` with explicit spec-driven conformance evidence and live validation artifacts.

## Scope
- Validate and harden inbound coalescing behavior.
- Preserve configurable coalescing windows and zero-window bypass.
- Ensure typing lifecycle signaling is emitted for coalesced batches.

## Issues
- Epic: #2507
- Story: #2508
- Task: #2509
- Subtask: #2510

## Exit Criteria
- #2509 AC matrix all green.
- RED/GREEN + mutation + live validation evidence posted in PR.
- G11 checklist items marked complete in `tasks/spacebot-comparison.md`.
