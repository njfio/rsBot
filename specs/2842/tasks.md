# Tasks: Issue #2842 - /ops/sessions/{session_key} detail timeline/validation/usage contracts

1. [ ] T1 (RED): add failing `functional_spec_2842_*` UI tests for detail panel/timeline/validation/usage contracts.
2. [ ] T2 (RED): add failing `functional_spec_2842_*` and `integration_spec_2842_*` gateway tests for `/ops/sessions/{session_key}` route behavior.
3. [ ] T3 (GREEN): implement `tau-dashboard-ui` detail snapshot structs + deterministic SSR markers.
4. [ ] T4 (GREEN): implement gateway detail-route wiring and session detail snapshot collection from `SessionStore`.
5. [ ] T5 (REGRESSION): rerun `spec_2838` and `spec_2834` suites and fix regressions.
6. [ ] T6 (VERIFY): run fmt/clippy/scoped tests/mutation and a fast live validation pass.
