# Tasks #2590

1. [x] T1 (verify): run mapped #2589 conformance/regression commands.
2. [x] T2 (verify): run scoped lint/format + crate tests.
3. [x] T3 (mutation): run `cargo mutants --in-diff` for touched Rust paths.
4. [x] T4 (live validation): run sanitized live smoke and capture summary.
5. [x] T5 (process): update issue logs and finalize closure evidence package.

## Evidence
- T1: `cargo test -p tau-tools spec_2589_c01_tool_policy_parses_memory_default_importance_overrides -- --test-threads=1`; `cargo test -p tau-memory spec_2589_c02_file_memory_store_applies_configured_type_default_importance -- --test-threads=1`; `cargo test -p tau-tools spec_2589_c03_memory_write_uses_configured_default_importance_profile -- --test-threads=1`; `cargo test -p tau-tools regression_2589_c04_memory_write_rejects_out_of_range_configured_defaults -- --test-threads=1` -> passed.
- T2: `cargo fmt --check`; `cargo clippy -p tau-memory -p tau-tools -- -D warnings`; `cargo test -p tau-memory --lib`; `cargo test -p tau-tools --lib` -> passed.
- T3: `cargo mutants --in-diff /tmp/issue2589.diff -p tau-memory -p tau-tools` -> `19 tested in 6m: 14 caught, 5 unviable, 0 missed`.
- T4: `TAU_PROVIDER_KEYS_FILE=/tmp/provider-keys-empty.env ./scripts/dev/provider-live-smoke.sh` -> `provider-live-smoke summary: ok=0 skipped=8 failed=0`.
