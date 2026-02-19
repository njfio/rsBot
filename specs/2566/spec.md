# Spec #2566 - Task: implement warn-tier background compaction scheduling and apply flow

Status: Implemented
Priority: P0
Milestone: M97
Parent: #2565

## Problem Statement
`tau-agent-core` currently compacts warn-tier context synchronously inside `request_messages`. That reduces context immediately, but it does not match the phase-3 objective for non-blocking warn-tier background compaction with deterministic apply/fallback behavior.

## Scope
- Add warn-tier compaction scheduling state to the agent runtime path.
- Schedule warn-tier compaction in the background without blocking the active turn.
- Apply ready compaction artifacts on subsequent turns when still relevant.
- Keep aggressive/emergency compaction behavior deterministic and backward compatible.
- Add conformance/regression coverage for scheduling, apply, and stale-result fallback paths.

## Out of Scope
- Memory extraction/persistence during compaction.
- Compaction via dedicated cross-session compactor process.
- Cortex, branch/worker orchestration, or transport changes.

## Acceptance Criteria
- AC-1: Given request context hits warn tier for the first time, when `request_messages` runs, then warn compaction is scheduled in background and the active request is not synchronously warn-truncated.
- AC-2: Given a ready background warn compaction artifact for the same context fingerprint, when the next request is prepared, then the artifact is applied and request messages include summary-style compaction.
- AC-3: Given a stale or unavailable background artifact, when request preparation runs, then runtime does not block, ignores stale results, and refreshes scheduling for current context.
- AC-4: Given context pressure reaches aggressive or emergency tier, when request preparation runs, then existing deterministic synchronous behavior is preserved (aggressive summary compaction and emergency hard truncation).

## Conformance Cases
- C-01 (AC-1, conformance): `spec_2566_c01_warn_tier_schedules_background_compaction_without_immediate_truncation`
- C-02 (AC-2, conformance): `spec_2566_c02_warn_tier_applies_ready_background_compaction_on_subsequent_turn`
- C-03 (AC-3, regression): `regression_spec_2566_c03_stale_warn_background_result_is_ignored_and_rescheduled`
- C-04 (AC-4, regression): `regression_spec_2566_c04_aggressive_tier_remains_synchronous_with_summary`
- C-05 (AC-4, regression): `regression_spec_2566_c05_emergency_tier_remains_hard_truncation_without_summary`

## Success Signals
- Warn-tier compaction no longer blocks first eligible turn.
- Ready warn-tier compaction is consumed deterministically on a subsequent turn.
- Aggressive/emergency behavior and existing token-pressure safety remain stable.
