# Tasks: Issue #3099 - ops memory-graph pan contracts

1. [x] T1 (RED): add failing `functional_spec_3099_*` UI tests for default pan markers and directional action links.
2. [x] T2 (RED): add failing `integration_spec_3099_*` gateway tests for query-driven clamped pan contracts.
3. [x] T3 (GREEN): implement deterministic pan query normalization and pan marker/action rendering.
4. [x] T4 (REGRESSION): rerun selected suites (`spec_3094`, `spec_3090`, `spec_3086`, `spec_3082`, `spec_3078`, `spec_3070`, `spec_3068`, `spec_3064`, `spec_3060`, `spec_2921`, `spec_2917`, `spec_2913`, `spec_2909`, `spec_2905`).
5. [x] T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`, and scoped spec suites.
