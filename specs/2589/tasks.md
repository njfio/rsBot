# Tasks #2589

1. [x] T1 (tests/red): add failing conformance/regression tests for configurable default-importance parsing + write fallback behavior.
2. [x] T2 (impl): implement policy/profile model and wire `FileMemoryStore` fallback to configured defaults.
3. [x] T3 (green): run C-01..C-04 conformance commands and update roadmap checklist status line (C-05).
4. [x] T4 (verify): run scoped fmt/clippy/tests for touched crates.
5. [x] T5 (process): update issue process logs and hand off closure evidence packaging to #2590.

## Evidence
- C-01: `cargo test -p tau-tools spec_2589_c01_tool_policy_parses_memory_default_importance_overrides -- --test-threads=1` -> passed.
- C-02: `cargo test -p tau-memory spec_2589_c02_file_memory_store_applies_configured_type_default_importance -- --test-threads=1` -> passed.
- C-03: `cargo test -p tau-tools spec_2589_c03_memory_write_uses_configured_default_importance_profile -- --test-threads=1` -> passed.
- C-04: `cargo test -p tau-tools regression_2589_c04_memory_write_rejects_out_of_range_configured_defaults -- --test-threads=1` -> passed.
- C-05: `tasks/spacebot-comparison.md` G5 configurable-defaults bullet updated to `[x]`.
- Verify: `cargo fmt --check`, `cargo clippy -p tau-memory -p tau-tools -- -D warnings`, `cargo test -p tau-memory --lib`, `cargo test -p tau-tools --lib` -> passed.
