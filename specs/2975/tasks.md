# Tasks: Issue #2975 - Session API runtime extraction

## Ordered Tasks
1. [x] T1 (RED): capture baseline hotspot-size check (`gateway_openresponses.rs` line count >= 2800).
2. [x] T2 (GREEN): extract session handlers/helpers into `session_api_runtime.rs` and wire imports.
3. [x] T3 (REGRESSION): run targeted gateway session endpoint tests.
4. [x] T4 (VERIFY): run fmt/clippy and confirm line-count threshold.

## Tier Mapping
- Unit: targeted `tau-gateway` session tests
- Property: N/A (no invariant algorithm changes)
- Contract/DbC: N/A (no contract semantics changed)
- Snapshot: N/A (no snapshot updates expected)
- Functional: session endpoint behavior remains unchanged
- Conformance: C-01..C-04
- Integration: route-table wiring to extracted session runtime
- Fuzz: N/A (no parser/untrusted input branch changes)
- Mutation: N/A (refactor-only logic move)
- Regression: targeted session endpoint tests
- Performance: N/A (no perf contract change)
