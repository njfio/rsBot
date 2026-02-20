# M112 - Cortex Observer Event Coverage Expansion

Status: Active

## Context
This milestone extends the Cortex observer work from M111 (`/cortex/chat` + `/cortex/status`) toward remaining G3 tracking parity from `tasks/spacebot-comparison.md`.

## Source
- `tasks/spacebot-comparison.md` (G3 Cortex cross-session observer)

## Objective
Expand deterministic Cortex observer event coverage in `tau-gateway` for memory-save and worker/session progress signals so operators can monitor broader runtime behavior through `/cortex/status`.

## Scope
- Add observer event instrumentation for selected memory-save operations.
- Add observer event instrumentation for selected worker/session progress operations.
- Verify expanded coverage with spec-derived conformance/regression tests.

## Issue Map
- Epic: #2706
- Story: #2707
- Task: #2708

## Acceptance Signals
- `/cortex/status` event counters include the new event classes under authenticated calls.
- Unauthorized/fallback behavior remains deterministic and fail-closed.
- Scoped gateway verification gates remain green.
