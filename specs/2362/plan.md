# Plan #2362

Status: Reviewed
Spec: specs/2362/spec.md

## Approach

1. Extend `AgentConfig` with tier threshold and retention parameters (warn,
   aggressive, emergency).
2. Add deterministic compaction helpers in `tau-agent-core`:
   - compute token-utilization from current message history.
   - choose compaction tier.
   - apply summary compaction for warn/aggressive tiers.
   - apply hard truncation for emergency tier.
3. Invoke tier compaction during request preparation before `ChatRequest`
   assembly.
4. Add conformance/regression tests in existing `tau-agent-core` test modules
   and verify prompt-path behavior under token pressure.

## Affected Modules (planned)

- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-agent-core/src/tests/structured_output_and_parallel.rs`
- `crates/tau-agent-core/src/tests/config_and_direct_message.rs` (if helper-level
  tests fit better)

## Risks and Mitigations

- Risk: over-aggressive compaction can remove too much context.
  - Mitigation: configurable retention and explicit tests per tier.
- Risk: compaction still fails budget checks for some edge cases.
  - Mitigation: run compaction before request assembly and validate with a
    pressure-path functional test.
- Risk: emergent behavior drift in existing bounded-message tests.
  - Mitigation: preserve defaults and add regression assertions for
    below-threshold behavior.
