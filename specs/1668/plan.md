# Issue 1668 Plan

Status: Reviewed

## Approach

1. Add a new `ppo` module in `tau-algorithm` with:
   - config model
   - typed sample input shape
   - loss-term output structures
   - deterministic loss/update APIs
2. Implement clipped-surrogate policy loss, value MSE term, and entropy bonus
   contribution with explicit finite-value guards.
3. Add update aggregation that chunks samples into minibatches and folds them
   into optimizer-step summaries based on configured accumulation steps.
4. Add deterministic unit/regression tests that verify exact expected values and
   fail-closed behavior for invalid inputs.

## Affected Areas

- `crates/tau-algorithm/src/lib.rs`
- `crates/tau-algorithm/src/ppo.rs` (new)
- `crates/tau-algorithm/Cargo.toml` (if instrumentation dependency wiring is needed)
- `specs/1668/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: floating-point drift causes flaky assertions.
  - Mitigation: use deterministic vectors with explicit tolerances.
- Risk: ambiguous gradient accumulation semantics.
  - Mitigation: define deterministic chunking and step-fold rules in tests.
- Risk: non-finite propagation under edge values.
  - Mitigation: validate all input/output numeric fields as finite and error
    early.

## ADR

No architecture/protocol change. ADR not required.
