# Issue 1970 Plan

Status: Reviewed

## Approach

1. Add `validate_exported_benchmark_artifact` helper in
   `benchmark_artifact.rs`.
2. The helper will:
   - read file contents
   - parse JSON payload
   - assert top-level object
   - assert `schema_version == 1`
   - assert required keys exist
3. Return validated `serde_json::Value` on success for downstream use.
4. Add tests C-01..C-04 and regression coverage.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1970/spec.md`
- `specs/1970/plan.md`
- `specs/1970/tasks.md`

## Risks And Mitigations

- Risk: validator drifts from export schema.
  - Mitigation: required-key list and schema-version checks anchored to
    `BenchmarkEvaluationArtifact::SCHEMA_VERSION_V1`.
- Risk: ambiguous errors for operators.
  - Mitigation: deterministic error messages with explicit missing key/version.

## ADR

No dependency/protocol changes; ADR not required.
