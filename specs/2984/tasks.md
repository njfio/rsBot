# Tasks: Issue #2984 - gateway config runtime extraction

1. [ ] T1 (RED): capture baseline line-count and run scoped config tests.
2. [ ] T2 (GREEN): extract config handlers + helper plumbing into `config_runtime.rs` and wire imports.
3. [ ] T3 (REGRESSION): rerun targeted config/gateway regression tests.
4. [ ] T4 (VERIFY): run fmt/clippy and confirm hotspot reduction.
5. [ ] T5 (VALIDATE): run sanitized fast live validation command.

## Tier Mapping
- Unit: targeted config endpoint tests.
- Property: N/A (no invariant algorithm change).
- Contract/DbC: N/A (no contract macro changes).
- Snapshot: N/A (no snapshot changes).
- Functional: config endpoint contract behavior.
- Conformance: C-01..C-05.
- Integration: route wiring + config handler integration.
- Fuzz: N/A (no new parser surface).
- Mutation: N/A (refactor-only move).
- Regression: scoped config + nearby gateway tests.
- Performance: N/A (no perf contract change).
