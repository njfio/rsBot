# Tasks: Issue #2901 - ops chat assistant token-stream rendering contracts

1. [x] T1 (RED): add failing `functional_spec_2901_*` UI tests for assistant token metadata + ordered token rows.
2. [x] T2 (RED): add failing `integration_spec_2901_*` gateway tests for persisted assistant token row coverage and non-assistant marker exclusion.
3. [x] T3 (GREEN): implement minimal chat assistant token row rendering contracts in `tau-dashboard-ui`.
4. [x] T4 (REGRESSION): rerun chat and sessions/detail suites (`spec_2830`, `spec_2834`, `spec_2872`, `spec_2881`, `spec_2862`, `spec_2866`, `spec_2870`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2885`, `spec_2889`, `spec_2893`, `spec_2897`).
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation + sanitized live validation.
