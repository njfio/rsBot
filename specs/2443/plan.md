# Plan #2443

Status: Reviewed
Spec: specs/2443/spec.md

## Approach

1. Add RED conformance tests for relation write/read/search/validation.
2. Implement relation persistence path in `tau-memory`.
3. Integrate relation-aware tool contracts in `tau-tools`.
4. Add graph scoring contribution in ranking pipeline.
5. Run verification gates including mutation testing for touched diff.

## Affected Modules

- `crates/tau-memory/src/runtime.rs`
- `crates/tau-memory/src/runtime/backend.rs`
- `crates/tau-memory/src/runtime/query.rs`
- `crates/tau-tools/src/tools.rs`
- `crates/tau-tools/src/tools/memory_tools.rs`
- `crates/tau-tools/src/tools/tests.rs`

## Risks and Mitigations

- Risk: ranking drift from additive graph score.
  - Mitigation: deterministic conformance + arithmetic-focused mutation tests.
- Risk: storage compatibility regressions.
  - Mitigation: additive schema and legacy read regression coverage.
