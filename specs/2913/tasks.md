# Tasks: Issue #2913 - ops memory type-filter narrowing contracts

1. [x] T1 (RED): add failing `functional_spec_2913_*` UI tests for type-filter markers/value preservation.
2. [x] T2 (RED): add failing `integration_spec_2913_*` gateway tests proving type-filter narrowing.
3. [x] T3 (GREEN): implement type-filter snapshot/control plumbing and narrowed result contracts.
4. [x] T4 (REGRESSION): rerun selected suites (`spec_2802`, `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2885`, `spec_2889`, `spec_2893`, `spec_2897`, `spec_2901`, `spec_2905`, `spec_2909`).
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation + sanitized live validation.
