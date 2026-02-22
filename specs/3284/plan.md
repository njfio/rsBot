# Plan: Issue #3284

## Approach
1. Define an internal `LiveRewardBreakdown` struct with deterministic scoring fields and `composite()` helper.
2. Replace scalar `compute_live_reward` with `compute_live_reward_breakdown`.
3. Preserve safety hard-gate by forcing composite `-1.0` when blocked.
4. Persist breakdown fields in `build_final_decision_span` attributes.
5. Add RED tests first for deterministic scoring and span attribute persistence.

## Affected Modules
- `crates/tau-coding-agent/src/live_rl_runtime.rs`

## Risks and Mitigations
- Risk: unintentionally changing existing reward semantics too aggressively.
  Mitigation: keep deterministic bounded components and explicit hard-gate override, assert expected values in tests.
- Risk: span attribute drift.
  Mitigation: conformance test that asserts required keys and values.

## Interfaces / Contracts
- Internal runtime contract only; no public API changes.
- Span attribute contract addition:
  - `reward_completion`
  - `reward_reliability`
  - `reward_safety`
  - `reward_efficiency`

## ADR
Not required (no dependency, protocol, or architecture boundary change).
