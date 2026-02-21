# Tasks: Issue #3103 - ops memory-graph filter contracts

1. [x] T1 (RED): add failing `functional_spec_3103_*` UI tests for default filter markers and query-driven filtered graph contracts.
2. [x] T2 (RED): add failing `integration_spec_3103_*` gateway tests for query-driven filter contracts.
3. [x] T3 (GREEN): implement filter query normalization and filter marker/action rendering.
4. [x] T4 (GREEN): apply filter state to memory graph node/edge contract views.
5. [x] T5 (REGRESSION): rerun selected suites (`spec_3099`, `spec_3094`, `spec_3090`, `spec_3086`, `spec_3082`, `spec_3078`, `spec_3070`, `spec_3068`, `spec_3064`, `spec_3060`, `spec_2921`, `spec_2917`, `spec_2913`, `spec_2909`, `spec_2905`).
6. [x] T6 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`, and scoped spec suites.
