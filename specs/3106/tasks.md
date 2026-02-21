# Tasks: Issue #3106 - ops tools inventory contracts

1. [x] T1 (RED): add failing `functional_spec_3106_*` UI tests for tools panel visibility, inventory summary markers, row markers, and hidden-state behavior.
2. [x] T2 (RED): add failing `integration_spec_3106_*` gateway test validating `/ops/tools-jobs` lists all registered tools.
3. [x] T3 (GREEN): extend dashboard snapshot data model with tools inventory row contracts and defaults.
4. [x] T4 (GREEN): render `/ops/tools-jobs` panel with deterministic inventory table/row markers.
5. [x] T5 (GREEN): populate tools inventory rows in gateway snapshot collection from registered tools.
6. [x] T6 (REGRESSION): rerun selected route/memory graph suites (`spec_3103`, `spec_3099`, `spec_2794`).
7. [x] T7 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`, and scoped spec suites.
