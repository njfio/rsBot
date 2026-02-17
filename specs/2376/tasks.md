# Tasks: Issue #2376

## T1 (Red): Conformance tests first
- [x] Add C-02 test in coding-agent runtime suite for two-prompt cumulative usage/cost.
- [x] Extend/adjust C-03 gateway test to assert cumulative estimated cost persistence semantics.
- [x] Ensure C-01/C-04 remain explicit and mapped in session tests.

## T2 (Green): Minimal implementation
- [x] Patch runtime/session code only if new conformance tests fail.
- [x] Keep diff scoped to session usage/cost behavior.

## T3 (Refactor)
- [x] Consolidate duplicated test helper logic for cumulative usage assertions where useful.
  No consolidation required beyond explicit conformance naming and assertions.

## T4 (Verify)
- [x] `cargo fmt --check`
- [x] `cargo clippy -p tau-session -p tau-coding-agent -p tau-gateway -- -D warnings`
- [x] Run targeted tests for C-01..C-04 mapping.
- [ ] `cargo mutants --in-diff <diff-file>` scoped to touched files.
  Blocked: in this workspace, `cargo-mutants` repeatedly hangs/interrupts on unmutated/baseline execution and scoped runs; follow-up issue required for deterministic mutation CI lane.
