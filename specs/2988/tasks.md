# Tasks: Issue #2988 - channel lifecycle and telemetry runtime extraction

1. [x] T1 (RED): capture baseline line-count and run scoped channel/telemetry tests.
2. [x] T2 (GREEN): extract handlers + helper plumbing into `channel_telemetry_runtime.rs` and wire imports.
3. [x] T3 (REGRESSION): rerun scoped channel/telemetry tests and nearby regressions.
4. [x] T4 (VERIFY): run fmt/clippy and confirm line-count threshold below 2000.
5. [x] T5 (VALIDATE): run sanitized fast live validation command.

## Tier Mapping
- Unit: targeted channel lifecycle and telemetry tests.
- Property: N/A (no invariant algorithm changes).
- Contract/DbC: N/A (no contract macro changes).
- Snapshot: N/A (no snapshot updates).
- Functional: endpoint behavior checks.
- Conformance: C-01..C-05.
- Integration: route wiring and file persistence integration.
- Fuzz: N/A (no parser surface changes).
- Mutation: N/A (refactor-only move).
- Regression: scoped endpoint regression reruns.
- Performance: N/A (no perf contract changes).
