# Plan #2041

Status: Reviewed
Spec: specs/2041/spec.md

## Approach

1. Complete planning subtask `#2060` to codify split boundaries, extraction
   order, and deterministic validation artifacts.
2. Execute extraction subtask `#2061` by moving benchmark artifact tests into
   `benchmark_artifact/tests.rs` and keeping root-module API behavior stable.
3. Validate with line-count guardrails plus conformance/regression/integration
   test evidence.

## Affected Modules

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `crates/tau-trainer/src/benchmark_artifact/tests.rs`
- `scripts/dev/test-benchmark-artifact-domain-split.sh`
- `scripts/dev/benchmark-artifact-split-map.sh` and companion docs/schemas

## Risks and Mitigations

- Risk: extraction could regress benchmark artifact serialization/reporting.
  - Mitigation: run targeted conformance slices and regression checks.
- Risk: split work could stall on long compile/test cycles.
  - Mitigation: run scoped tests with isolated target dir (`--target-dir
    target-fast`) and targeted filters.

## Interfaces and Contracts

- Public benchmark artifact APIs remain unchanged.
- Split-map contract from `#2060` governs decomposition boundaries.

## ADR References

- Not required.
