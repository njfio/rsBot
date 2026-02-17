# Tasks: Issue #2379

## T1 (Red): Conformance tests first
- [x] Add C-01 test: session-cost QA-loop config parses/validates and remains scoped.
- [x] Add C-02 test: QA-loop fails fast and does not execute later stages after first failure.
- [x] Add C-03 test: operator doc includes canonical invocation contract.

## T2 (Green): Minimal implementation
- [x] Add canonical QA-loop config at `docs/qa/session-cost-mutation.qa-loop.json`.
- [x] Add operator runbook at `docs/qa/session-cost-mutation-lane.md`.

## T3 (Refactor)
- [x] Keep command strings centralized and readable (no duplicated inline magic in tests).

## T4 (Verify)
- [x] `cargo fmt --check`
- [x] `cargo clippy -p tau-ops -- -D warnings`
- [x] `cargo test -p tau-ops spec_c01_session_cost_mutation_config_is_valid_and_scoped -- --nocapture`
- [x] `cargo test -p tau-ops spec_c02_session_cost_mutation_lane_stops_after_first_failed_stage -- --nocapture`
- [x] `cargo test -p tau-ops spec_c03_session_cost_mutation_docs_include_canonical_invocation -- --nocapture`
