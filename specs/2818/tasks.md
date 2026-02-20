# Tasks: Issue #2818 - Command-center alert feed list SSR markers

## Ordered Tasks
1. [x] T1 (RED): add failing UI + gateway conformance tests for alert feed list row markers and nominal fallback row.
2. [x] T2 (GREEN): extend `tau-dashboard-ui` command-center alert feed rendering for row markers.
3. [x] T3 (GREEN): extend gateway command-center snapshot mapping to provide deterministic alert row list context.
4. [x] T4 (REGRESSION): run phase-1A..1I command-center regression suites.
5. [x] T5 (VERIFY): run fmt/clippy/tests/mutation/guardrails and set spec status to `Implemented`.

## Tier Mapping
- Unit: alert row mapping defaults + render behavior.
- Property: N/A.
- Contract/DbC: N/A.
- Snapshot: N/A.
- Functional: alert row marker assertions.
- Conformance: C-01..C-04.
- Integration: gateway `/ops` render with dashboard fixtures.
- Fuzz: N/A.
- Mutation: `cargo mutants --in-diff <diff-file> -p tau-gateway -p tau-dashboard-ui`.
- Regression: phase-1A..1I contract suites.
- Performance: N/A.

## Verification Evidence
- `cargo fmt --check` ✅
- `cargo clippy -p tau-dashboard-ui -p tau-gateway -- -D warnings` ✅
- `cargo test -p tau-dashboard-ui functional_spec_2818 -- --test-threads=1` ✅
- `cargo test -p tau-gateway functional_spec_2818 -- --test-threads=1` ✅
- `cargo test -p tau-gateway functional_collect_tau_ops_dashboard_command_center_snapshot_maps_dashboard_snapshot -- --test-threads=1` ✅
- `cargo test -p tau-gateway functional_spec_{2786,2794,2798,2802,2806,2810,2814} -- --test-threads=1` ✅
- `cargo test -p tau-dashboard-ui` ✅
- `cargo test -p tau-gateway` ✅
- `python3 .github/scripts/oversized_file_guard.py` ✅
- `cargo mutants --in-diff /tmp/mutants_2818.diff -p tau-gateway -p tau-dashboard-ui` ✅ (`3 tested, 3 caught, 0 escaped`)
