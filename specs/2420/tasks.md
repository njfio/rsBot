# Tasks: Issue #2420 - Slack bridge message coalescing

## Ordered Tasks
1. T1 (RED): add C-01/C-02/C-03 Slack runtime conformance tests and C-04 onboarding config test.
2. T2 (GREEN): implement queue coalescing helper and integrate in `try_start_queued_runs`.
3. T3 (GREEN): add coalescing config field through CLI -> onboarding -> runtime wiring.
4. T4 (GREEN): update default CLI fixture in coding-agent tests.
5. T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-slack-runtime -- -D warnings`, `cargo test -p tau-slack-runtime`, and targeted onboarding tests.
6. T6 (CLOSE): PR with AC mapping, RED/GREEN evidence, and completed tier matrix.

## Tier Mapping
- Unit: queue helper boundary checks
- Functional: C-01/C-02 queue-to-run behavior
- Regression: C-03 non-coalescible tail preservation
- Integration: C-04 startup config propagation
- Property/Contract/Snapshot/Fuzz/Mutation/Performance: N/A for this scoped transport behavior slice
