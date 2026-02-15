# Issue 1674 Plan

Status: Reviewed

## Approach

1. Extend `crates/tau-trainer/src/benchmark_significance.rs` with:
   - summary-statistics output struct
   - baseline-vs-candidate comparison report struct
   - deterministic confidence interval and significance helpers
2. Add JSON serialization helper for machine-readable report artifacts.
3. Add tests-first conformance coverage for:
   - deterministic summary metrics
   - comparative significance behavior
   - report JSON contract and invalid input regression checks
4. Run scoped formatting/lint/tests and map ACs to conformance cases in PR.

## Affected Areas

- `crates/tau-trainer/src/benchmark_significance.rs`
- `specs/1674/spec.md`
- `specs/1674/plan.md`
- `specs/1674/tasks.md`

## Risks And Mitigations

- Risk: confidence interval math ambiguity (sample vs population variance).
  - Mitigation: document formula assumptions directly in rustdoc and tests.
- Risk: overclaiming significance with tiny samples.
  - Mitigation: enforce minimum sample count validation and explicit CI math.

## ADR

No dependency or wire-format change; ADR not required.
