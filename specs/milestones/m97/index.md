# M97 - Spacebot G2 Context Compaction (Phase 3)

Milestone: GitHub milestone `M97 - Spacebot G2 Context Compaction (Phase 3)`

## Objective
Implement warn-tier background compaction orchestration so high-context sessions can schedule non-blocking summary work before aggressive/emergency truncation is required.

## Scope
- Add warn-tier compaction scheduling that runs in background and does not block the active turn.
- Use scheduled summary artifact on subsequent turns when available.
- Keep deterministic fallback behavior if background summary is unavailable/failed.
- Conformance and regression coverage for scheduling + application behavior.

## Out of Scope
- Memory extraction/saving during compaction.
- Cross-session compactor/cortex architecture.
- Additional transport/channel behavior changes.

## Issue Hierarchy
- Epic: #2564
- Story: #2565
- Task: #2566
- Subtask: #2567

## Exit Criteria
- Warn-tier background compaction scheduling is implemented and test-covered.
- Existing aggressive/emergency tier behavior remains deterministic and backward compatible.
- Conformance + mutation + live-validation evidence is packaged in subtask #2567.
