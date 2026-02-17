# Tasks: Issue #2405 - Restore fail-closed OpenResponses preflight budget gate

## Ordered Tasks

1. T1 (RED): Reproduce gateway preflight failures
   - Run C-01 and C-02 tests and capture failing output.

2. T2 (GREEN): Patch preflight budget configuration
   - Update OpenResponses agent config wiring so preflight budget rejection remains fail-closed.

3. T3 (GREEN): Verify conformance + regression
   - Run C-01/C-02/C-03 targeted tests.
   - Run scoped `cargo fmt --check`.
   - Run scoped `cargo clippy -p tau-gateway -- -D warnings`.

4. T4 (VERIFY): Run critical-gap gateway verification
   - Validate gateway leg in `scripts/dev/verify-critical-gaps.sh`.

5. T5 (CLOSE): Lifecycle and PR evidence
   - Update status logs/labels.
   - Open PR with AC mapping, RED/GREEN evidence, and tier matrix.

## Tier Mapping
- Unit: N/A (gateway integration-path behavior)
- Functional: C-01
- Conformance: C-01/C-02/C-03
- Integration: C-01/C-02/C-03 gateway tests
- Regression: C-03
- Property/Contract/Snapshot/Fuzz/Performance: N/A for this scoped fix
- Mutation: N/A (single config-wire change; no critical algorithm branch change)
