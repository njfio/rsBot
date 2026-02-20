# M111 - Cortex Admin Chat API Foundation

Status: In Progress
Source gap: `tasks/spacebot-comparison.md` (G3: Cortex cross-session observer)

## Objective
Deliver initial Cortex runtime API primitives in `tau-gateway` by introducing a deterministic admin chat SSE endpoint that provides an operator-facing observer surface and establishes contracts for later Cortex expansion.

## Planned Issue Map
- Epic: #2699
- Story: #2700
- Task: #2701

## Deliverables
- Cortex admin chat endpoint contract foundation:
  - `POST /cortex/chat` (SSE)
- Authenticated fail-closed behavior and deterministic validation errors.
- Status discovery metadata in gateway status payload for Cortex endpoint.
- Conformance and regression tests for happy path and failure modes.

## Exit Criteria
- Milestone epic and scoped story/task issues are closed.
- Issue spec status is `Implemented`.
- Scoped verification gates pass with evidence.
- `tasks/spacebot-comparison.md` G3 progress reflects implemented slice.
