# Tasks: Issue #2830 - Chat message send and transcript visibility contracts

## Ordered Tasks
1. [x] T1 (RED): add/extend conformance tests for chat send-form/transcript contracts in `tau-dashboard-ui` and `tau-gateway`.
2. [x] T2 (GREEN): implement gateway chat snapshot hydration + `POST /ops/chat/send` append/redirect behavior.
3. [x] T3 (REGRESSION): run targeted ops shell regression suites.
4. [x] T4 (VERIFY): run fmt/clippy/mutation/guardrails and set spec status to `Implemented`.

## Tier Mapping
- Unit: `ops_shell_controls` session query parsing unit tests.
- Property: N/A.
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: UI `/ops/chat` marker assertions.
- Conformance: C-01..C-03.
- Integration: gateway send + redirect + transcript visibility assertions.
- Fuzz: N/A.
- Mutation: `cargo mutants --in-diff <diff-file> -p tau-dashboard-ui -p tau-gateway`.
- Regression: targeted existing ops-shell suites.
- Performance: N/A.

## Verification Evidence
- Targeted:
  - `cargo test -p tau-dashboard-ui functional_spec_2830 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2830 -- --test-threads=1`
  - `cargo test -p tau-gateway integration_spec_2830 -- --test-threads=1`
  - `cargo test -p tau-gateway unit_requested_session_key -- --test-threads=1`
- Regression:
  - `cargo test -p tau-dashboard-ui functional_spec_2826 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2802 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2826 -- --test-threads=1`
- Verify:
  - `cargo fmt --check`
  - `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
  - `python3 .github/scripts/oversized_file_guard.py`
  - `cargo mutants --in-diff /tmp/mutants_2830.diff -p tau-dashboard-ui -p tau-gateway` (`6/6` caught)
  - `cargo test -p tau-dashboard-ui`
  - `cargo test -p tau-gateway`
  - `cargo test` (workspace run; unrelated existing failures in `tau-coding-agent`, no failures in touched crates)
