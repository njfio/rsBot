# Tasks: Issue #2614 - Build production dashboard UI (G18) with auth and live status views

## Ordered Tasks
1. T1 (RED): add failing webchat-render test assertions for dashboard tab/live controls.
2. T2 (GREEN): implement dashboard panel HTML/JS + authenticated fetch/action/live polling wiring.
3. T3 (VERIFY): run scoped fmt/clippy/tests for gateway dashboard surface.
4. T4 (CLOSE): update issue process log and open PR with AC map + tier matrix.

## Tier Mapping
- Unit: C-01
- Property: N/A (UI wiring + deterministic payload rendering)
- Contract/DbC: N/A (no contracts macro usage)
- Snapshot: N/A (string/JSON assertions are explicit)
- Functional: C-02
- Conformance: C-01..C-06
- Integration: C-03, C-04, C-05
- Fuzz: N/A (no new parser or untrusted format)
- Mutation: N/A (P2 UI integration slice; non-critical algorithm path)
- Regression: C-04, C-05 (preserve existing dashboard endpoint/action/stream behavior)
- Performance: N/A (bounded operator-driven polling; no benchmark contract change)
