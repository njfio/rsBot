# Spec #2463 - G16 hot-reload config phase-1 orchestration

Status: Implemented

## Problem Statement
`tasks/spacebot-comparison.md` identifies `G16` as a gap: Tau runtime configuration changes require process restart. This orchestration issue defines the first bounded delivery slice for runtime hot-reload behavior.

## Acceptance Criteria
### AC-1 Orchestration scope is explicit and bounded
Given phase-1 delivery for G16, when implementation is planned, then scope is limited to runtime heartbeat policy hot-reload with clear out-of-scope boundaries.

### AC-2 Child work products are complete and traceable
Given milestone M79, when implementation is complete, then story/task/subtask artifacts exist with AC-linked conformance tests and closure evidence.

## Scope
In scope:
- M79 issue hierarchy and binding artifacts.
- Runtime heartbeat policy hot-reload slice.

Out of scope:
- Full profile-wide hot-reload.
- New dependency adoption (`notify`, `arc-swap`) in this slice.

## Conformance Cases
- C-01 (AC-1, governance): M79 index + #2464/#2465/#2466 specs exist and describe bounded scope.
- C-02 (AC-2, governance): #2465 contains AC->test mapping and RED/GREEN verification evidence.

## Success Metrics
- Milestone M79 closes with all child issues closed.
- #2465 AC matrix has no failing entries.
