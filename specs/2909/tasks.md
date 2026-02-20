# Tasks: Issue #2909 - ops memory scope-filter narrowing contracts

1. [x] T1 (RED): add failing `functional_spec_2909_*` UI tests for workspace/channel/actor filter markers and value preservation.
2. [x] T2 (RED): add failing `integration_spec_2909_*` gateway tests proving scope-filter narrowing by workspace/channel/actor.
3. [x] T3 (GREEN): implement scope-filter snapshot/form controls and narrowed search contract plumbing.
4. [x] T4 (REGRESSION): rerun selected suites (`spec_2802`, `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2885`, `spec_2889`, `spec_2893`, `spec_2897`, `spec_2901`, `spec_2905`).
5. [x] T5 (VERIFY): run fmt/clippy/scoped tests/mutation + sanitized live validation.
