# Tasks: Issue #3012 - Review #31 stale crate reference correction

## Ordered Tasks
1. [x] T1 (RED): extend review conformance script with stale crate-name assertions and capture failure.
2. [x] T2 (GREEN): patch Review #31 under-tested crate rows to valid crates/signals.
3. [x] T3 (REGRESSION): rerun conformance script.
4. [x] T4 (VERIFY): run `cargo fmt --check` and `cargo check -q`.

## Tier Mapping
- Unit: shell assertion helpers in review conformance script
- Property: N/A (no algorithmic changes)
- Contract/DbC: N/A (no API/DbC changes)
- Snapshot: N/A (no snapshots)
- Functional: doc conformance script
- Conformance: C-01, C-02, C-03, C-04
- Integration: N/A (docs/script only)
- Fuzz: N/A (no parser fuzz changes)
- Mutation: N/A (docs/script only)
- Regression: conformance script rerun after fix
- Performance: N/A (no runtime perf changes)
