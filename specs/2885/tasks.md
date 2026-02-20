# Tasks: Issue #2885 - session branch creation and lineage contracts

1. [x] T1 (RED): add failing `functional_spec_2885_*` UI test for sessions detail row-level branch form contracts.
2. [x] T2 (RED): add failing `functional_spec_2885_*` + `integration_spec_2885_*` gateway tests for branch endpoint, redirect contracts, and branch-limited transcript behavior.
3. [x] T3 (GREEN): implement branch form SSR markers, gateway branch handler, and router wiring for `/ops/sessions/branch`.
4. [x] T4 (REGRESSION): rerun `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2872`, and `spec_2881` suites.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation + fast live validation.
