# Spec #2362

Status: Implemented
Milestone: specs/milestones/m59/index.md
Issue: https://github.com/njfio/Tau/issues/2362

## Problem Statement

`tasks/spacebot-comparison.md` gap G2 calls out missing tiered context
compaction behavior. Current `tau-agent-core` compacts only by
`max_context_messages`, but does not react to token-utilization pressure before
budget overflow.

## Scope

In scope:

- Add token-utilization tier thresholds to `AgentConfig`.
- Apply tiered compaction in request shaping:
  - warn tier: retain ~70% (drop ~30%) with deterministic summary.
  - aggressive tier: retain ~50% with deterministic summary.
  - emergency tier: hard truncate oldest ~50% without summary insertion.
- Keep behavior deterministic and testable through existing unit/integration
  harnesses.

Out of scope:

- Background compactor workers.
- LLM-based summarization tasks.
- Cross-session/global Cortex behavior.

## Acceptance Criteria

- AC-1: Given estimated input-token utilization at or above warn threshold and
  below aggressive threshold, when request shaping runs, then history compaction
  applies warn retention and keeps summary-style compaction behavior.
- AC-2: Given utilization at or above aggressive threshold and below emergency
  threshold, when request shaping runs, then aggressive retention is applied.
- AC-3: Given utilization at or above emergency threshold, when request shaping
  runs, then emergency hard truncation is applied without context-summary
  insertion.
- AC-4: Given utilization below warn threshold, when request shaping runs, then
  no tier compaction path is applied (existing bounded behavior only).
- AC-5: Given pressure-triggered compaction configuration in a live prompt path,
  when the agent runs, then request generation stays within configured
  token-budget limits instead of failing immediately from oversized history.

## Conformance Cases

- C-01 (AC-1, unit): warn-tier compaction path selected and message retention
  reduced to configured warn fraction.
- C-02 (AC-2, unit): aggressive-tier compaction path selected and retention
  reduced to configured aggressive fraction.
- C-03 (AC-3, regression): emergency tier strips oldest context with no summary
  prefix message.
- C-04 (AC-4, regression): below-threshold history does not trigger tiered
  compaction path.
- C-05 (AC-5, functional/integration): prompt path under token pressure uses
  compaction and does not return `TokenBudgetExceeded` for the configured case.
