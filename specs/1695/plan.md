# Issue 1695 Plan

Status: Reviewed

## Approach

1. Add a PPO reference fixture JSON under `crates/tau-algorithm/testdata/ppo/`
   that includes:
   - config
   - input samples
   - expected loss terms
   - tolerance
2. Extend PPO tests to parse fixture vectors and assert field-by-field
   conformance within tolerance thresholds.
3. Add deterministic regression coverage for update aggregation metrics on
   clipping-edge values.

## Affected Areas

- `crates/tau-algorithm/src/ppo.rs`
- `crates/tau-algorithm/testdata/ppo/`
- `specs/1695/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: brittle fixture parsing.
  - Mitigation: include strict validation with descriptive field errors.
- Risk: floating-point noise creates flaky failures.
  - Mitigation: case-level tolerances and deterministic vectors only.

## ADR

No architecture/protocol/dependency boundary change. ADR not required.
