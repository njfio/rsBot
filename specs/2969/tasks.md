# Tasks: Issue #2969 - External coding-agent runtime extraction

## Ordered Tasks
1. [x] T1 (RED): capture baseline hotspot-size check (`gateway_openresponses.rs` line count > phase-1 target).
2. [x] T2 (GREEN): extract external coding-agent handlers/helpers into `external_agent_runtime.rs` and wire imports.
3. [x] T3 (REGRESSION): run targeted external coding-agent gateway tests.
4. [x] T4 (VERIFY): run fmt/clippy scoped checks and confirm line-count reduction.

## Tier Mapping
- Unit: existing gateway unit/functional tests
- Property: N/A (no invariant/algorithm change)
- Contract/DbC: N/A (no API contract semantics change)
- Snapshot: N/A (no snapshot changes expected)
- Functional: external coding-agent endpoint behavior
- Conformance: C-01..C-04
- Integration: cross-module routing + runtime bridge handlers
- Fuzz: N/A (no parser/untrusted input logic changes)
- Mutation: N/A (refactor-only, no business-logic branch changes)
- Regression: targeted external coding-agent tests
- Performance: N/A (no perf contract change)
