# Tasks: Issue #2881 - chat multi-line input contracts

1. [x] T1 (RED): add failing `functional_spec_2881_*` UI test for multiline compose marker contracts.
2. [x] T2 (RED): add failing `functional_spec_2881_*` + `integration_spec_2881_*` gateway tests for newline preservation and hidden-route contracts.
3. [x] T3 (GREEN): implement additive multiline compose markers and newline-preserving send-path behavior.
4. [x] T4 (REGRESSION): rerun `spec_2830`, `spec_2834`, `spec_2858`, `spec_2862`, `spec_2866`, `spec_2870`, and `spec_2872` suites.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation + fast live validation.
