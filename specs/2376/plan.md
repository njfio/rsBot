# Plan: Issue #2376

## Approach
1. Reuse existing session usage persistence primitives in `tau-session`.
2. Add/extend conformance tests at runtime call-sites to prove cumulative behavior.
3. Patch runtime/session logic only if conformance tests expose drift.
4. Run scoped quality gates and mutation on touched diff.

## Affected Modules
- `crates/tau-session/src/tests.rs`
- `crates/tau-coding-agent/src/tests/auth_provider/runtime_and_startup.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`

## Risks and Mitigations
- Risk: floating-point comparison fragility for cost values.
  Mitigation: assert epsilon-based equality and monotonic checks.
- Risk: runtime fixtures masking persistence regressions.
  Mitigation: always reload `SessionStore` from disk for assertions.

## Interfaces / Contracts
- `SessionStore::record_usage_delta(SessionUsageSummary)` remains additive and monotonic.
- `run_prompt_with_cancellation` and gateway OpenResponses session runtime must persist per-turn deltas.
- `execute_session_stats_command` must reflect persisted cumulative usage.

## ADR
No ADR required; no dependency/protocol/architecture change.
