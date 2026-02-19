# Tasks: Issue #2619 - External coding-agent bridge protocol staging (G21)

## Ordered Tasks
1. T1 (RED): add failing tests for session lifecycle/pool reuse, event streaming order, and timeout reaping.
2. T2 (GREEN): implement external coding-agent bridge runtime module and exports.
3. T3 (DOC): add ADR for protocol boundary and lifecycle semantics.
4. T4 (VERIFY): run scoped fmt/clippy/tests and map AC/C evidence.
5. T5 (CLOSE): update issue process log and open PR with tier matrix + TDD evidence.

## Tier Mapping
- Unit: C-01, C-01b
- Property: N/A (deterministic state-machine staging scope)
- Contract/DbC: N/A (no contracts macros)
- Snapshot: N/A (explicit structured assertions)
- Functional: C-02
- Conformance: C-01, C-01b, C-02..C-05
- Integration: C-02 (bridge behavior across lifecycle + event APIs)
- Fuzz: N/A (no new untrusted parser surface)
- Mutation: N/A (non-critical-path staged runtime scaffolding)
- Regression: C-03
- Performance: N/A (no performance SLO change)
