# Plan #2061

Status: Reviewed
Spec: specs/2061/spec.md

## Approach

1. Create failing threshold/parity tests for benchmark artifact decomposition.
2. Extract high-volume benchmark domains (schema/IO/report/validation) into
   dedicated modules under `benchmark_artifact/`.
3. Keep public APIs and serialization contracts stable via root re-exports.
4. Run benchmark conformance + integration checks and record evidence.

## Affected Modules

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `crates/tau-trainer/src/benchmark_artifact/*`
- benchmark artifact split/guardrail test scripts and contract tests

## Risks and Mitigations

- Risk: schema/reporting regressions in artifact outputs.
  - Mitigation: keep conformance tests and serialized-output assertions green.
- Risk: broad extraction introduces import cycles.
  - Mitigation: execute phased modules from split map and use explicit
    re-exports.

## Interfaces and Contracts

- Public benchmark artifact interfaces remain stable for trainer callers.
- Split-map contract from `#2060` governs module boundaries and migration order.

## ADR References

- Not required.
