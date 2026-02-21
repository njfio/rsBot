# Tasks: Issue #3068 - ops memory-graph nodes and edges contracts

1. [x] T1 (RED): add failing `functional_spec_3068_*` UI tests for graph panel/list defaults and hidden route behavior.
2. [x] T2 (RED): add failing `integration_spec_3068_*` gateway tests for node/edge row hydration on `/ops/memory-graph`.
3. [x] T3 (GREEN): implement memory-graph context rows and deterministic SSR marker rendering.
4. [x] T4 (REGRESSION): rerun selected memory suites (`spec_2905`, `spec_2909`, `spec_2913`, `spec_2917`, `spec_2921`, `spec_3060`, `spec_3064`).
5. [x] T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`, and scoped spec suites.
