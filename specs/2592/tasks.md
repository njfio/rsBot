# Tasks #2592

1. [x] T1 (tests/red): add failing conformance/regression tests for relation enum, canonical normalization, BFS traversal, and graph-in-RRF fusion.
2. [x] T2 (impl): implement typed relation enum and canonical relation normalization/backfill parsing.
3. [x] T3 (impl): implement bounded BFS graph traversal scoring and RRF fusion integration.
4. [x] T4 (green): run C-01..C-05 conformance commands and update G6 checklist bullets (C-06).
5. [x] T5 (verify): run scoped fmt/clippy/tests and mutation-in-diff for touched Rust paths.
6. [x] T6 (process): update issue process logs and hand off closure evidence packaging to #2593.

## Evidence
- C-01: `cargo test -p tau-memory spec_2592_c01_memory_relation_enum_canonical_roundtrip -- --test-threads=1` -> passed.
- C-02: `cargo test -p tau-memory spec_2592_c02_normalize_relations_accepts_only_supported_relation_enum_values -- --test-threads=1` -> passed.
- C-03: `cargo test -p tau-tools spec_2592_c03_memory_write_read_roundtrip_canonical_relation_type -- --test-threads=1` -> passed.
- C-04: `cargo test -p tau-memory spec_2592_c04_search_graph_bfs_expands_two_hop_relation_paths -- --test-threads=1` -> passed.
- C-05: `cargo test -p tau-memory regression_2592_c05_search_fuses_graph_ranking_via_rrf_path -- --test-threads=1` -> passed.
- C-06: `tasks/spacebot-comparison.md` G6 relation enum / BFS traversal / graph-in-RRF bullets updated to `[x]`.
- Verify: `cargo fmt --check`; `cargo clippy -p tau-memory -p tau-tools -- -D warnings`; `cargo test -p tau-memory --lib`; `cargo test -p tau-tools --lib` -> passed.
- Mutation: `cargo mutants --in-diff /tmp/issue2592.diff -p tau-memory -p tau-tools` -> `60 tested in 8m: 54 caught, 6 unviable, 0 missed`.
