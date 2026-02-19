# M99 - Spacebot G2 Context Compaction (Phase 5 LLM Warn Summarization)

Status: In Progress
Related roadmap item: `tasks/spacebot-comparison.md` -> G2 remaining 80% warn-tier LLM summarize pathway

## Objective
Complete the remaining G2 compaction gap by executing warn-tier (80%) background compaction using LLM-generated summary artifacts while preserving deterministic fallback, non-blocking behavior, and existing aggressive/emergency guarantees.

## Issue Map
- Epic: #2577
- Story: #2578
- Task: #2579
- Subtask: #2580

## Deliverables
- Warn-tier background compaction uses LLM summarization for dropped context.
- Deterministic fallback summary is used when LLM summarization fails/times out.
- Conformance + regression + mutation + sanitized live validation evidence package.

## Exit Criteria
- #2579 and #2580 closed.
- `specs/2579/spec.md` and `specs/2580/spec.md` status set to `Implemented`.
- G2 80% warn-tier pathway checklist item updated in `tasks/spacebot-comparison.md`.
