# Tasks: Issue #2872 - chat new-session creation contracts

1. [ ] T1 (RED): add failing `functional_spec_2872_*` UI tests for new-session form markers.
2. [ ] T2 (RED): add failing `functional_spec_2872_*` + `integration_spec_2872_*` gateway tests for create+redirect+selector+hidden-route contracts.
3. [ ] T3 (GREEN): implement additive UI new-session form markers and gateway `POST /ops/chat/new` behavior.
4. [ ] T4 (REGRESSION): rerun `spec_2830`, `spec_2834`, `spec_2858`, `spec_2862`, `spec_2866`, and `spec_2870` suites.
5. [ ] T5 (VERIFY): run fmt/clippy/scoped tests/mutation + fast live validation.
