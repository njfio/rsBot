# Tasks #2593

1. [x] T1 (verify): run mapped #2592 conformance/regression commands.
2. [x] T2 (verify): run scoped lint/format + crate tests.
3. [x] T3 (mutation): run `cargo mutants --in-diff` for touched Rust paths.
4. [x] T4 (live validation): run sanitized live smoke and capture summary.
5. [x] T5 (process): update issue logs and finalize closure evidence package.

## Evidence
- T1: `cargo test -p tau-memory spec_2592_c01_memory_relation_enum_canonical_roundtrip -- --test-threads=1`; `cargo test -p tau-memory spec_2592_c02_normalize_relations_accepts_only_supported_relation_enum_values -- --test-threads=1`; `cargo test -p tau-tools spec_2592_c03_memory_write_read_roundtrip_canonical_relation_type -- --test-threads=1`; `cargo test -p tau-memory spec_2592_c04_search_graph_bfs_expands_two_hop_relation_paths -- --test-threads=1`; `cargo test -p tau-memory regression_2592_c05_search_fuses_graph_ranking_via_rrf_path -- --test-threads=1` -> passed.
- T2: `cargo fmt --check`; `cargo clippy -p tau-memory -p tau-tools -- -D warnings`; `cargo test -p tau-memory --lib`; `cargo test -p tau-tools --lib` -> passed.
- T3: `cargo mutants --in-diff /tmp/issue2592.diff -p tau-memory -p tau-tools` -> `60 tested in 8m: 54 caught, 6 unviable, 0 missed`.
- T4: `TAU_PROVIDER_KEYS_FILE=/tmp/provider-keys-empty.env ./scripts/dev/provider-live-smoke.sh` -> `provider-live-smoke summary: ok=0 skipped=8 failed=0`.
