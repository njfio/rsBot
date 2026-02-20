# Tasks: Issue #2889 - session reset confirmation and clear-session contracts

1. [x] T1 (RED): add failing `functional_spec_2889_*` UI tests for session detail reset-confirmation form contract markers.
2. [x] T2 (RED): add failing `functional_spec_2889_*` + `integration_spec_2889_*` gateway tests for reset action, redirect contracts, cleared detail view, and non-target isolation.
3. [x] T3 (GREEN): implement reset form SSR markers and ops reset POST handler on session detail route.
4. [x] T4 (REGRESSION): rerun `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2872`, `spec_2881`, and `spec_2885` suites.
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation + fast live validation.
