# Tasks: Issue #2897 - session detail complete message coverage contracts

1. [x] T1 (RED): add failing `functional_spec_2897_*` UI tests for detail timeline count/coverage markers.
2. [x] T2 (RED): add failing `integration_spec_2897_*` gateway tests for complete non-empty message coverage + empty-content exclusion.
3. [x] T3 (GREEN): harden detail rendering only if RED exposes behavior gaps.
4. [x] T4 (REGRESSION): rerun `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2885`, `spec_2889`, `spec_2893`.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation + fast live validation.
