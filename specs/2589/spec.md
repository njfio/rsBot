# Spec #2589 - Task: implement G5 runtime-configurable memory-type default importance profile

Status: Reviewed
Priority: P1
Milestone: M101
Parent: #2588

## Problem Statement
G5 parity remains incomplete because memory-type default importance values are compile-time constants (`MemoryType::default_importance`) rather than runtime/profile-configurable values.

## Scope
- Add a configurable memory-type default importance profile to runtime tool policy.
- Wire `FileMemoryStore` write fallback (`importance=None`) to that profile.
- Preserve existing validation guarantees (0.0..=1.0 finite bounds).
- Update G5 checklist parity line after conformance validation.

## Out of Scope
- G6 relation-model parity changes (enum/BFS/3-way RRF).
- New embedding/search architecture changes unrelated to default-importance configuration.

## Acceptance Criteria
- AC-1: Tool policy resolves/exports per-memory-type default importance configuration with bounded validation.
- AC-2: `FileMemoryStore` applies configured per-type defaults when writes omit explicit importance.
- AC-3: `memory_write` tool uses configured defaults while preserving explicit-override behavior and validation failures.
- AC-4: `tasks/spacebot-comparison.md` G5 configurable-defaults checkbox reflects validated implementation status.

## Conformance Cases
- C-01 (AC-1, conformance): `cargo test -p tau-tools spec_2589_c01_tool_policy_parses_memory_default_importance_overrides -- --test-threads=1`
- C-02 (AC-2, conformance): `cargo test -p tau-memory spec_2589_c02_file_memory_store_applies_configured_type_default_importance -- --test-threads=1`
- C-03 (AC-3, functional): `cargo test -p tau-tools spec_2589_c03_memory_write_uses_configured_default_importance_profile -- --test-threads=1`
- C-04 (AC-3, regression): `cargo test -p tau-tools regression_2589_c04_memory_write_rejects_out_of_range_configured_defaults -- --test-threads=1`
- C-05 (AC-4, process): G5 configurable-defaults checklist bullet updated in `tasks/spacebot-comparison.md`

## Success Signals
- Operators can tune default importance without code changes.
- Memory writes with omitted `importance` reflect configured type defaults deterministically.
