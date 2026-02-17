# Tasks: Issue #2400 - Stabilize startup model-catalog remote refresh assertion

## Ordered Tasks

1. T1 (RED): Reproduce brittle failure
   - Run `cargo test -p tau-coding-agent --test cli_integration integration_startup_model_catalog_remote_refresh_is_reported -- --nocapture`.
   - Capture failing output demonstrating `entries=1` mismatch.

2. T2 (GREEN): Update assertion contract
   - Edit `integration_startup_model_catalog_remote_refresh_is_reported`.
   - Keep remote-source assertions; replace exact count assertion with numeric match.

3. T3 (GREEN): Verify conformance + regression
   - Re-run targeted test.
   - Run scoped `cargo fmt --check`.
   - Run scoped `cargo clippy -p tau-coding-agent --tests -- -D warnings`.

4. T4 (CLOSE): Lifecycle updates
   - Update issue status/phase comments.
   - Open PR with AC mapping, RED/GREEN evidence, and test-tier matrix.

## Tier Mapping
- Unit: N/A (integration-only change)
- Functional: C-02 via targeted integration assertion checks
- Conformance: C-01/C-02/C-03 via targeted integration test
- Integration: targeted `cli_integration` test
- Regression: rerun same test after patch to confirm stability
- Property/Contract/Snapshot/Fuzz/Performance: N/A for this slice
- Mutation: N/A for this test-only assertion fix (no production logic mutation target)
