# Plan: Issue #3156 - Property invariants for rate-limit reset, disable, and payload contracts

## Approach
1. Add RED property tests for reset, disable, and gate-payload invariants in `tau-tools`.
2. Validate RED failure behavior where assertions are stricter than current assumptions.
3. Adjust tests to match invariant intent while preserving strict contract assertions.
4. Run scoped verification (`spec_3156`, regression `spec_3152`, fmt, clippy).

## Affected Modules
- `crates/tau-tools/src/tools/tests.rs`

## Risks & Mitigations
- Risk: flakiness from wall-clock usage in gate-level helper.
  - Mitigation: assert bounded/relative time contract fields, not exact timestamps.
- Risk: over-constraining payload shape beyond contract.
  - Mitigation: assert required field presence/semantics only.

## Interfaces / Contracts
- `ToolPolicy::evaluate_rate_limit(...)`
- `evaluate_tool_rate_limit_gate(...)`

## ADR
No ADR required.
