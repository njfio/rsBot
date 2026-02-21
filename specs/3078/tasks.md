# Tasks: Issue #3078 - ops memory-graph node color type contracts

1. [x] T1 (RED): add failing `functional_spec_3078_*` UI tests for node-color marker contracts.
2. [x] T2 (RED): add failing `integration_spec_3078_*` gateway tests for multi-type node-color mapping.
3. [x] T3 (GREEN): implement deterministic memory-type color marker rendering.
4. [x] T4 (REGRESSION): rerun selected suites (`spec_3070`, `spec_3068`, `spec_3064`, `spec_3060`, `spec_2921`, `spec_2917`, `spec_2913`, `spec_2909`, `spec_2905`).
5. [x] T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`, and scoped spec suites.
