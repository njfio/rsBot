# Plan #2579

## Approach
1. Introduce warn-tier async compaction worker path that can request LLM summary text for dropped messages.
2. Keep scheduling/apply state machine intact (`pending` -> `ready`) and non-blocking for active turn.
3. Implement deterministic fallback summary generator for failed/invalid LLM responses.
4. Add conformance/regression tests before implementation and tighten assertions for tier isolation.
5. Verify with scoped gates plus mutation/live validation handoff in #2580.

## Affected Modules
- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-agent-core/src/runtime_turn_loop.rs`
- `crates/tau-agent-core/src/tests/structured_output_and_parallel.rs`

## Risks & Mitigations
- Risk: background LLM calls can add nondeterminism/latency.
  - Mitigation: keep asynchronous warn path and strict fallback summary path.
- Risk: malformed LLM response weakens summary contract.
  - Mitigation: validate summary prefix/shape and fallback on parse/shape mismatch.
- Risk: aggressive/emergency regressions via shared helpers.
  - Mitigation: preserve tier isolation and keep regression coverage for non-warn paths.

## Interfaces / Contracts
- Internal `tau-agent-core` request-shaping contract only.
- No external API/wire format changes expected.
