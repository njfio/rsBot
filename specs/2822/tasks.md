# Tasks: Issue #2822 - Command-center connector health table SSR markers

## Ordered Tasks
1. [x] T1 (RED): add failing UI + gateway conformance tests for connector row markers and fallback row.
2. [x] T2 (GREEN): extend `tau-dashboard-ui` command-center rendering for connector table row markers.
3. [x] T3 (GREEN): extend gateway command-center snapshot mapping with multi-channel connector rows.
4. [x] T4 (REGRESSION): run phase-1A..1J command-center regression suites.
5. [x] T5 (VERIFY): run fmt/clippy/tests/mutation/guardrails and set spec status to `Implemented`.

## Tier Mapping
- Unit: connector row mapping + fallback behavior.
- Property: N/A.
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: connector row marker assertions.
- Conformance: C-01..C-04.
- Integration: gateway `/ops` render with multi-channel connector fixtures.
- Fuzz: N/A.
- Mutation: `cargo mutants --in-diff <diff-file> -p tau-gateway -p tau-dashboard-ui`.
- Regression: phase-1A..1J contract suites.
- Performance: N/A.

## Verification Evidence
- RED:
  - `cargo test -p tau-dashboard-ui functional_spec_2822 -- --test-threads=1` (failed before markers existed)
  - `cargo test -p tau-gateway functional_spec_2822 -- --test-threads=1` (failed before connector table markers existed)
- GREEN + regression:
  - `cargo test -p tau-dashboard-ui functional_spec_2822 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2822 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_collect_tau_ops_dashboard_command_center_snapshot_maps_dashboard_snapshot -- --test-threads=1`
  - `cargo test -p tau-dashboard-ui functional_spec_2786 -- --test-threads=1`
  - `cargo test -p tau-dashboard-ui functional_spec_2794 -- --test-threads=1`
  - `cargo test -p tau-dashboard-ui functional_spec_2798 -- --test-threads=1`
  - `cargo test -p tau-dashboard-ui functional_spec_2806 -- --test-threads=1`
  - `cargo test -p tau-dashboard-ui functional_spec_2810 -- --test-threads=1`
  - `cargo test -p tau-dashboard-ui functional_spec_2814 -- --test-threads=1`
  - `cargo test -p tau-dashboard-ui functional_spec_2818 -- --test-threads=1`
  - `cargo test -p tau-dashboard-ui functional_spec_2822 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2786 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2794 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2798 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2802 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2806 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2810 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2814 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2818 -- --test-threads=1`
  - `cargo test -p tau-gateway functional_spec_2822 -- --test-threads=1`
- Verify:
  - `cargo fmt --check`
  - `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings`
  - `cargo test -p tau-dashboard-ui`
  - `cargo test -p tau-gateway`
  - `python3 .github/scripts/oversized_file_guard.py`
  - `cargo mutants --in-diff /tmp/mutants_2822.diff -p tau-gateway -p tau-dashboard-ui` (`3/3` caught)
  - `cargo test` (workspace run shows unrelated pre-existing failures in `tau-coding-agent`; touched crates from this issue remain green)
