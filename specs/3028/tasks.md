# Tasks: Issue #3028 - Publish crate dependency architecture diagram

## Ordered Tasks
1. [x] T1 (RED): add conformance test for dependency graph script contract and capture failing output.
2. [x] T2 (GREEN): implement deterministic dependency graph script and emit report artifacts.
3. [x] T3 (GREEN): add architecture doc with command and artifact contract.
4. [x] T4 (REGRESSION): run conformance test and generate live workspace artifacts.
5. [x] T5 (VERIFY): run `cargo fmt --check` and `cargo check -q`.

## Tier Mapping
- Unit: shell assertions for script output/schema
- Property: N/A (no randomized invariant path)
- Contract/DbC: N/A (no Rust API contract changes)
- Snapshot: N/A (no snapshot harness)
- Functional: dependency graph generation and doc contract checks
- Conformance: C-01, C-02, C-03, C-04
- Integration: N/A (no runtime cross-module behavior change)
- Fuzz: N/A (no untrusted runtime parser surface)
- Mutation: N/A (docs/script slice)
- Regression: rerun conformance + baseline checks
- Performance: N/A (no runtime performance path changes)
