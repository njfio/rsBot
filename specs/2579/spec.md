# Spec #2579 - Task: implement warn-tier background LLM summarization compaction path

Status: Implemented
Priority: P0
Milestone: M99
Parent: #2578

## Problem Statement
Warn-tier background compaction currently summarizes dropped context with deterministic local compaction text. The remaining G2 requirement calls for LLM-based summarization at the 80% warn threshold while keeping non-blocking scheduling/apply behavior and fail-safe semantics.

## Scope
- Run warn-tier background compaction through an LLM summary generation path.
- Preserve existing asynchronous schedule/apply semantics.
- Add deterministic local fallback summary when LLM summarization cannot be produced.
- Keep aggressive/emergency behavior unchanged.
- Add conformance/regression tests for warn-tier LLM/fallback behavior.

## Out of Scope
- New process architecture or cross-session compactor services.
- Changing aggressive/emergency compaction algorithms.
- New external provider requirements beyond existing agent model client contract.

## Acceptance Criteria
- AC-1: Given warn-tier pressure, when background compaction is scheduled, then summary generation uses an LLM-backed pathway and remains non-blocking for the current turn.
- AC-2: Given ready warn compaction output, when applied on subsequent turn, then resulting context includes a summary artifact derived from LLM output.
- AC-3: Given LLM summarization fails or times out, when warn compaction runs, then deterministic fallback summary is used and request flow remains fail-safe.
- AC-4: Given aggressive/emergency pressure tiers, when compaction runs, then existing synchronous aggressive and hard-truncation emergency behavior remains unchanged.

## Conformance Cases
- C-01 (AC-1, conformance): `spec_2579_c01_warn_tier_schedules_background_llm_compaction_without_immediate_truncation`
- C-02 (AC-2, conformance): `spec_2579_c02_warn_tier_applies_ready_llm_summary_compaction_on_subsequent_turn`
- C-03 (AC-2, functional): `spec_2579_c03_warn_llm_summary_includes_structured_context_prefix`
- C-04 (AC-3, regression): `regression_spec_2579_c04_warn_llm_summary_failure_falls_back_to_deterministic_summary`
- C-05 (AC-4, regression): `regression_spec_2579_c05_aggressive_emergency_paths_remain_unchanged`

## Success Signals
- Warn-tier background compaction no longer depends solely on local deterministic summarization.
- Failure paths preserve deterministic behavior without panics.
- Existing tiered compaction behavior remains stable for non-warn tiers.
