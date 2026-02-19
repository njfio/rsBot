# Tasks: Issue #2616 - OpenTelemetry export path for production observability

## Ordered Tasks
1. T1 (RED): add failing tests for CLI/config propagation and runtime/gateway OpenTelemetry export behavior.
2. T2 (GREEN): add CLI OpenTelemetry export flag/env and propagate into onboarding runtime/gateway config builders.
3. T3 (GREEN): implement prompt telemetry OpenTelemetry export records (trace + metrics) behind opt-in path.
4. T4 (GREEN): implement gateway cycle OpenTelemetry export records (trace + metrics) behind opt-in path.
5. T5 (VERIFY): run scoped fmt/clippy/tests and confirm AC/C mapping.
6. T6 (CLOSE): update issue process log, open PR with tier matrix and TDD evidence.

## Tier Mapping
- Unit: C-01
- Property: N/A (no parser/invariant randomization added)
- Contract/DbC: N/A (no new contracts annotations)
- Snapshot: N/A (structured assertions are sufficient)
- Functional: C-02
- Conformance: C-01..C-05
- Integration: C-04
- Fuzz: N/A (no new untrusted parser surface)
- Mutation: N/A (non-critical observability plumbing slice; deterministic regression coverage provided)
- Regression: C-03
- Performance: N/A (opt-in write path only, no hotspot contract changes)
