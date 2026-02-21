# Tasks: Issue #3090 - ops memory-graph hover highlight contracts

1. [ ] T1 (RED): add failing `functional_spec_3090_*` UI tests for hover highlight marker contracts.
2. [ ] T2 (RED): add failing `integration_spec_3090_*` gateway tests for focused-memory highlight mappings.
3. [ ] T3 (GREEN): implement deterministic edge/node highlight marker rendering.
4. [ ] T4 (REGRESSION): rerun selected suites (`spec_3086`, `spec_3082`, `spec_3078`, `spec_3070`, `spec_3068`, `spec_3064`, `spec_3060`, `spec_2921`, `spec_2917`, `spec_2913`, `spec_2909`, `spec_2905`).
5. [ ] T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`, and scoped spec suites.
