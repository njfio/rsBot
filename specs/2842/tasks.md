# Tasks: Issue #2842 - /ops/sessions/{session_key} detail timeline/validation/usage contracts

1. [x] T1 (RED): add failing `functional_spec_2842_*` UI tests for detail panel/timeline/validation/usage contracts.
2. [x] T2 (RED): add failing `functional_spec_2842_*` and `integration_spec_2842_*` gateway tests for `/ops/sessions/{session_key}` route behavior.
3. [x] T3 (GREEN): implement `tau-dashboard-ui` detail snapshot structs + deterministic SSR markers.
4. [x] T4 (GREEN): implement gateway detail-route wiring and session detail snapshot collection from `SessionStore`.
5. [x] T5 (REGRESSION): rerun `spec_2838` and `spec_2834` suites and fix regressions.
6. [x] T6 (VERIFY): run fmt/clippy/scoped tests/mutation and a fast live validation pass.

Verification summary:
- `cargo fmt --check`
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
- `python3 .github/scripts/oversized_file_guard.py`
- `cargo test -p tau-dashboard-ui`
- `cargo test -p tau-gateway`
- `cargo mutants --in-diff /tmp/mutants_2842.diff -p tau-dashboard-ui -p tau-gateway` -> 11 caught, 18 unviable, 0 missed
- `./scripts/dev/fast-validate.sh --skip-fmt --check-only --direct-packages-only --full`
