# Tasks: Issue #2638 - Gateway external coding-agent APIs and SSE stream (G21 phase 2)

## Ordered Tasks
1. T1 (RED): add failing gateway tests for C-01..C-05 before implementation.
2. T2 (GREEN): wire bridge state/config and implement lifecycle/follow-up/reap endpoints.
3. T3 (GREEN): implement SSE replay endpoint and gateway status metadata section.
4. T4 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, `cargo test -p tau-gateway`.
5. T5 (CLOSE): update issue status/process log and prepare PR with AC/tier/TDD evidence.

## Tier Mapping
- Unit: endpoint parsing/helpers + bridge summary helpers
- Property: N/A (no randomized parser/invariant surface)
- Contract/DbC: N/A (no contracts macros in this module)
- Snapshot: N/A (behavior asserted structurally)
- Functional: C-01, C-02
- Conformance: C-01..C-06
- Integration: C-01..C-05 (HTTP handlers + bridge state + auth)
- Fuzz: N/A (no new untrusted parser beyond serde_json already covered by malformed-json tests)
- Mutation: N/A (non-critical-path transport integration)
- Regression: C-05
- Performance: N/A (no explicit throughput SLO change)
