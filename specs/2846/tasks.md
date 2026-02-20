# Tasks: Issue #2846 - /ops/sessions/{session_key} session graph node/edge contracts

1. [ ] T1 (RED): add failing `functional_spec_2846_*` UI tests for graph panel, summary counts, node/edge rows, and empty state.
2. [ ] T2 (RED): add failing `functional_spec_2846_*` and `integration_spec_2846_*` gateway tests for `/ops/sessions/{session_key}` graph contracts.
3. [ ] T3 (GREEN): implement `tau-dashboard-ui` graph marker structs + deterministic SSR graph rendering.
4. [ ] T4 (GREEN): implement gateway graph snapshot derivation from selected session lineage parent links.
5. [ ] T5 (REGRESSION): rerun `spec_2842`, `spec_2838`, and `spec_2834` suites and fix regressions.
6. [ ] T6 (VERIFY): run fmt/clippy/scoped tests/mutation and fast live validation.
