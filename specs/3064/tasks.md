# Tasks: Issue #3064 - ops memory detail embedding and relations contracts

1. [ ] T1 (RED): add failing `functional_spec_3064_*` UI tests for detail panel
   markers, default hidden state, and deterministic marker IDs.
2. [ ] T2 (RED): add failing `integration_spec_3064_*` gateway tests for selected
   detail embedding metadata and relation row rendering.
3. [ ] T3 (GREEN): implement selected-memory detail flow and detail-panel marker
   rendering contracts.
4. [ ] T4 (REGRESSION): rerun selected memory specs
   (`spec_2905`, `spec_2909`, `spec_2913`, `spec_2917`, `spec_2921`, `spec_3060`).
5. [ ] T5 (VERIFY): run `cargo fmt --check`,
   `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`, and
   scoped spec suites.
