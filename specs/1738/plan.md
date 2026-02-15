# Issue 1738 Plan

Status: Reviewed

## Approach

1. Extend `tau-trainer::benchmark_significance` with:
   - safety regression threshold policy
   - checkpoint promotion gate evaluator returning deterministic reason codes
2. Add unit/integration/regression tests for allow/deny behavior based on:
   - safety regression threshold
   - seeded/sample-size reproducibility gate booleans
3. Add `tau-runtime` structured audit payload helper for checkpoint promotion
   decisions and test payload shape stability.

## Affected Areas

- `crates/tau-trainer/src/benchmark_significance.rs`
- `crates/tau-runtime/src/observability_loggers_runtime.rs`
- `specs/1738/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: ambiguous gate failures.
  - Mitigation: emit deterministic reason codes and computed regression values.
- Risk: policy misconfiguration.
  - Mitigation: fail closed on non-finite/negative threshold inputs.

## ADR

No architecture/dependency/protocol change. ADR not required.
