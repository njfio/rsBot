# Tasks: Issue #3112 - ops tools detail contracts

1. [x] T1 (RED): add failing `functional_spec_3112_*` UI tests for tool detail panel visibility and detail contract markers.
2. [x] T2 (RED): add failing `integration_spec_3112_*` gateway tests for `/ops/tools-jobs` tool detail markers.
3. [x] T3 (GREEN): extend dashboard snapshot data model with tool detail contract fields and default values.
4. [x] T4 (GREEN): render deterministic tool detail panel/metadata/policy/histogram/invocation markers on `/ops/tools-jobs`.
5. [x] T5 (GREEN): populate tool detail snapshot values from gateway tool registry and deterministic runtime defaults.
6. [x] T6 (REGRESSION): rerun selected suites (`spec_3106`, `spec_3103`, `spec_3099`, `spec_2794`).
7. [x] T7 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`, and scoped spec suites.
