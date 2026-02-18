# Tasks #2542

1. [x] T1: run RED for C-01..C-04.
2. [x] T2: run GREEN for C-01..C-04 after implementation.
3. [x] T3: execute fmt/clippy/scoped/full test gates.
4. [x] T4: execute `cargo mutants --in-diff` on touched crates.
5. [x] T5: run live validation script and capture outcome.
6. [x] T6: prepare PR evidence matrix.

Note: workspace `cargo test` is currently blocked by reproducible `tau-runtime` heartbeat hot-reload test failures unrelated to files touched for `#2541`.
