# M105 - Spacebot Remaining Gap Integration Wave 1

Status: In Progress
Related roadmap items: `tasks/spacebot-comparison.md` (Tier 5 G21 remaining integration)

## Objective
Implement the next unresolved Spacebot parity slice with full spec-driven/TDD execution, starting with G21 phase 2: authenticated gateway HTTP+SSE integration on top of Tau's external coding-agent bridge runtime contracts.

## Issue Map
- Epic: #2636
- Story: #2637
- Task: #2638

## Deliverables
- Authenticated gateway external coding-agent session lifecycle endpoints.
- Follow-up queue routing endpoints for interactive continuation.
- SSE progress replay endpoint backed by ordered runtime bridge events.
- Gateway status payload metadata/runtime snapshot for external coding-agent integration.
- AC/conformance-mapped tests and scoped verification evidence.

## Exit Criteria
- #2636, #2637, and #2638 closed.
- `specs/2638/spec.md` status set to `Implemented`.
- `tasks/spacebot-comparison.md` reflects current G21 status with evidence links.
