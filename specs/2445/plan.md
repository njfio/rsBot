# Plan #2445

Status: Reviewed
Spec: specs/2445/spec.md

## Approach

1. Add RED conformance tests in `tau-tools` mapped to C-01..C-05.
2. Validate GREEN pass after feature implementation.
3. Add targeted `tau-memory` runtime/query tests to close escaped mutants.
4. Re-run diff-scoped mutation gate until zero missed mutants.

## Affected Modules

- `crates/tau-tools/src/tools/tests.rs`
- `crates/tau-memory/src/runtime.rs`
- `crates/tau-memory/src/runtime/query.rs`

## Risks and Mitigations

- Risk: mutation survivors in arithmetic paths.
  - Mitigation: direct equation/branch tests for graph score composition.
- Risk: fragile test assertions tied to incidental counts/order.
  - Mitigation: assert required identifiers/metadata and deterministic formulas.
