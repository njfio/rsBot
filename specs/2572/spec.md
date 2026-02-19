# Spec #2572 - Task: implement compaction entry persistence and memory extraction/save

Status: Implemented
Priority: P0
Milestone: M98
Parent: #2571

## Problem Statement
Phase-3 introduced warn-tier background compaction scheduling/apply behavior, but remaining G2 scope still lacks two key behaviors: persisted compaction entries in session history and memory extraction/save during warn/aggressive compaction.

## Scope
- Persist compaction summaries as explicit entries in agent session history.
- Extract memory candidates from warn/aggressive compaction summaries.
- Save extracted memory candidates through existing memory/runtime pathways with fail-safe behavior.
- Add conformance/regression coverage for persistence/extraction flows.

## Out of Scope
- New cross-session compactor/cortex process architecture.
- New memory graph schema/types.
- Transport/channel behavior changes.

## Acceptance Criteria
- AC-1: Given warn/aggressive compaction produces a summary artifact, when request flow applies compaction, then a deterministic compaction entry is persisted in session history.
- AC-2: Given a persisted compaction summary, when extraction logic runs, then memory candidates are generated and routed to memory-save pathways.
- AC-3: Given extraction/save failure, when compaction flow continues, then agent behavior remains fail-safe (no panic, request flow continues deterministically).
- AC-4: Given emergency compaction tier, when request flow runs, then existing hard-truncation behavior remains unchanged and does not attempt summary memory extraction.

## Conformance Cases
- C-01 (AC-1, conformance): `spec_2572_c01_warn_compaction_persists_compaction_entry`
- C-02 (AC-1, conformance): `spec_2572_c02_aggressive_compaction_persists_compaction_entry`
- C-03 (AC-2, functional): `spec_2572_c03_compaction_summary_extracts_memory_candidates`
- C-04 (AC-3, regression): `regression_spec_2572_c04_memory_save_failure_does_not_break_request_flow`
- C-05 (AC-4, regression): `regression_spec_2572_c05_emergency_compaction_skips_summary_extraction`

## Success Signals
- Compaction entries become first-class persisted artifacts.
- Memory extraction/save happens deterministically on warn/aggressive compaction paths.
- Existing emergency behavior remains backward compatible.
