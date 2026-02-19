# Plan #2566

## Approach
1. Add warn-tier background compaction state to `Agent` using interior mutability so `request_messages(&self)` can poll/apply/schedule without blocking.
2. Split compaction path into tier selection + tier-specific compaction function so warn-tier results can be produced asynchronously and applied later.
3. In request preparation:
   - detect context pressure tier,
   - poll pending warn result,
   - apply ready result if fingerprint matches current context,
   - otherwise schedule warn compaction and continue without synchronous warn truncation.
4. Preserve aggressive/emergency synchronous compaction paths as-is.
5. Add/adjust conformance tests first (RED), then implement (GREEN), then stabilize assertions and naming.

## Affected Modules
- `crates/tau-agent-core/src/lib.rs`
- `crates/tau-agent-core/src/runtime_turn_loop.rs`
- `crates/tau-agent-core/src/tests/structured_output_and_parallel.rs`

## Risks & Mitigations
- Risk: stale background results applied to changed message history.
  - Mitigation: fingerprint request-context messages and only apply matching artifacts.
- Risk: excessive rescheduling churn.
  - Mitigation: track pending fingerprint and avoid duplicate schedule for identical context.
- Risk: regressions in aggressive/emergency compaction behavior.
  - Mitigation: keep existing code path and add regression conformance tests.

## Interfaces / Contracts
- Internal agent request-shaping contract for compaction tiers in `request_messages`.
- No external API or wire-format change.
