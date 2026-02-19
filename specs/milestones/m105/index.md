# M105 - Spacebot Remaining Gap Integration Wave 1

Status: In Progress
Related roadmap items: `tasks/spacebot-comparison.md` (Tier 5 G21/G22 remaining integration)

## Objective
Implement remaining high-impact Spacebot parity slices in sequence with full spec-driven/TDD execution:
1. G21 phase 2 authenticated gateway HTTP+SSE integration on top of Tau's external coding-agent bridge runtime contracts.
2. G22 SKILL.md compatibility routing parity (channel summary prompts, worker/delegated full prompts).

## Issue Map
- Epic: #2636
- Story (completed): #2637
- Task (completed): #2638
- Story (current): #2641
- Task (current): #2642

## Deliverables
- G21: authenticated gateway external coding-agent session lifecycle endpoints.
- G21: follow-up queue routing endpoints for interactive continuation.
- G21: SSE progress replay endpoint backed by ordered runtime bridge events.
- G21: gateway status payload metadata/runtime snapshot for external coding-agent integration.
- G22: SKILL.md frontmatter compatibility validation with `{baseDir}` substitution conformance coverage.
- G22: summary-mode skill composition for channel/system startup prompts.
- G22: full-skill context injection for worker/delegated orchestration prompts.
- AC/conformance-mapped tests and scoped verification evidence for each task.

## Exit Criteria
- #2638 and #2642 closed under epic #2636.
- `specs/2638/spec.md` and `specs/2642/spec.md` statuses set to `Implemented`.
- `tasks/spacebot-comparison.md` reflects current G21 and G22 status with evidence links.
