# Tasks: Issue #2429 - G4 phase-1 branch tool implementation and validation

## Ordered Tasks
1. T1 (RED): add C-01 test asserting built-in registry/name coverage for `branch`.
2. T2 (RED): add C-02/C-03 tests for successful branch append + explicit parent targeting.
3. T3 (RED): add C-04/C-05 regression tests for unknown parent and empty prompt validation.
4. T4 (GREEN): implement `BranchTool` with structured success/error payload contracts.
5. T5 (GREEN): wire tool registration in built-in tool list/registry.
6. T6 (VERIFY): run `cargo fmt --check`, scoped `clippy`, and `cargo test -p tau-tools`.
7. T7 (CLOSE): update issue process logs/status and conformance evidence mapping.

## Tier Mapping
- Unit: C-01
- Functional: C-02
- Conformance: C-01..C-05
- Integration: C-03
- Regression: C-04, C-05
- Property: N/A (no new randomized invariant surface)
- Contract/DbC: N/A (no new DbC annotations in this slice)
- Snapshot: N/A (no stable snapshot output in this slice)
- Fuzz: N/A (no new untrusted parser surface beyond existing guarded argument parsing)
- Mutation: N/A for iterative loop (run on critical paths during pre-PR gate if required)
- Performance: N/A (no hot-path algorithm change)
