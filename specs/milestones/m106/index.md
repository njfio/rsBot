# M106 - Spacebot G21 External Worker Subprocess Integration

Status: In Progress
Related roadmap items: `tasks/spacebot-comparison.md` (G21 remaining subprocess worker item)

## Objective
Close the remaining G21 parity gap by wiring real external coding-agent subprocess execution into Tau's external coding-agent bridge runtime while preserving existing gateway HTTP+SSE lifecycle contracts.

## Issue Map
- Epic: #2645
- Story: #2646
- Task: #2647

## Deliverables
- Extend external coding-agent bridge runtime config with optional subprocess launch configuration.
- Launch and supervise one subprocess worker per active external coding-agent session when subprocess mode is enabled.
- Stream subprocess stdout/stderr lines into ordered bridge progress events consumable via existing SSE replay endpoint.
- Forward follow-up messages into subprocess stdin while preserving existing queued follow-up behavior.
- Ensure close/reap lifecycle paths terminate active subprocesses safely and maintain deterministic session snapshots.
- Add conformance-mapped tests for spawn/reuse, stdin follow-up routing, stdout/stderr replay, and close/reap termination behavior.

## Exit Criteria
- #2645, #2646, and #2647 closed.
- `specs/2647/spec.md` status set to `Implemented`.
- G21 remaining checkbox in `tasks/spacebot-comparison.md` marked complete with issue evidence.
- Scoped verification gates pass and are recorded in PR evidence.
