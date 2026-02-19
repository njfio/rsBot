# Tasks #2584

1. [x] T1 (verify): run mapped conformance/regression commands for G5/G6/G7 criteria.
2. [x] T2 (audit): map checklist bullets to implementation/tests and identify any real gaps.
3. [x] T3 (docs): update `tasks/spacebot-comparison.md` checkboxes for validated items.
4. [x] T4 (verify): run scoped checks for touched crates/files.
5. [x] T5 (process): update issue process logs and hand off evidence packaging to #2585.

## Evidence

- C-01 passed: `cargo test -p tau-memory unit_memory_type_default_importance_profile_and_record_defaults -- --test-threads=1`
- C-02 passed (explicit filters):
  - `cargo test -p tau-tools spec_c01_memory_write_applies_type_default_importance -- --test-threads=1`
  - `cargo test -p tau-tools spec_c02_memory_write_rejects_invalid_importance_override -- --test-threads=1`
  - `cargo test -p tau-tools spec_c03_memory_read_and_search_include_type_and_importance -- --test-threads=1`
  - `cargo test -p tau-tools spec_c04_memory_search_boosts_higher_importance_records -- --test-threads=1`
- C-03 passed: `cargo test -p tau-tools spec_2444_ -- --test-threads=1`
- C-04 passed: `cargo test -p tau-memory integration_search_score_uses_vector_importance_and_graph_signal_additively -- --test-threads=1`
- C-05 passed:
  - `cargo test -p tau-memory spec_2455_ -- --test-threads=1`
  - `cargo test -p tau-memory spec_2450_c02_read_and_search_touch_lifecycle_metadata -- --test-threads=1`
  - `cargo test -p tau-memory 2460 -- --test-threads=1`
  - `cargo test -p tau-runtime integration_spec_2460_c03_runtime_heartbeat_executes_memory_lifecycle_maintenance -- --test-threads=1`
- Scoped quality gates passed:
  - `cargo fmt --check`
  - `cargo clippy -p tau-memory -p tau-tools -p tau-runtime -- -D warnings`
- Process log: `https://github.com/njfio/Tau/issues/2584#issuecomment-3925491250`

## Checklist Audit Notes

- G5 remaining gap: per-type default importance is implemented but not operator-configurable at runtime/profile level.
- G6 remaining gaps: relation model is validated but not Spacebot-parity 6-type enum; search uses deterministic graph signal add-on rather than BFS + 3-way RRF fusion.
