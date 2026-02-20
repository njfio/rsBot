# Tasks: Issue #2905 - ops memory search relevant result contracts

1. [x] T1 (RED): add failing `functional_spec_2905_*` UI tests for memory panel/form/query/result/empty-state markers.
2. [x] T2 (RED): add failing `integration_spec_2905_*` gateway tests seeding persisted memory entries and asserting relevant `/ops/memory` rows.
3. [x] T3 (GREEN): implement minimal memory search snapshot plumbing + memory panel rendering markers.
4. [x] T4 (REGRESSION): rerun selected chat/session/detail/ops suites (`spec_2802`, `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2885`, `spec_2889`, `spec_2893`, `spec_2897`, `spec_2901`).
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation + sanitized live validation.
