# Plan #2063

Status: Implemented
Spec: specs/2063/spec.md

## Approach

1. Add failing guardrail checks for `tools.rs` split threshold + module marker.
2. Extract high-volume tools runtime domain code from `tools.rs` into
   `crates/tau-tools/src/tools/` modules while preserving public exports.
3. Keep behavior stable by minimizing API signature changes and retaining
   policy/result contracts.
4. Run targeted tau-tools and integration tests; capture closure evidence.

## Affected Modules

- `crates/tau-tools/src/tools.rs`
- `crates/tau-tools/src/tools/*`
- split guardrail scripts and contract tests under `scripts/dev/`

## Risks and Mitigations

- Risk: extraction changes policy gate semantics.
  - Mitigation: keep gate helpers intact and run targeted regression tests.
- Risk: broad imports/regressions due module boundary moves.
  - Mitigation: use focused extraction of one large domain chunk first and
    preserve root re-exports.

## Interfaces and Contracts

- Keep existing public tool struct names/trait impls stable for callers.
- Preserve command input/output JSON and error envelope contracts.

## ADR References

- Not required.
