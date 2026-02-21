# Tasks: Issue #3094 - ops memory-graph zoom in/out contracts

1. [ ] T1 (RED): add failing `functional_spec_3094_*` UI tests for zoom markers and action links.
2. [ ] T2 (RED): add failing `integration_spec_3094_*` gateway tests for query-driven clamped zoom contracts.
3. [ ] T3 (GREEN): implement deterministic zoom query normalization and zoom marker rendering.
4. [ ] T4 (REGRESSION): rerun selected suites (`spec_3090`, `spec_3086`, `spec_3082`, `spec_3078`, `spec_3070`, `spec_3068`, `spec_3064`, `spec_3060`, `spec_2921`, `spec_2917`, `spec_2913`, `spec_2909`, `spec_2905`).
5. [ ] T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`, and scoped spec suites.
