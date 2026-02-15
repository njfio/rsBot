# Issue 1966 Plan

Status: Reviewed

## Approach

1. Add a new `benchmark_artifact` module to `tau-trainer`.
2. Introduce `BenchmarkEvaluationArtifact` plus a builder function that accepts:
   - benchmark metadata (suite/policy ids, generated timestamp)
   - `PolicyImprovementReport`
   - optional `SeedReproducibilityReport`
   - optional `SampleSizeSensitivityReport`
   - `CheckpointPromotionDecision`
3. Add deterministic `to_json_value()` serialization that emits explicit schema
   version and `null` optional sections.
4. Add conformance tests C-01..C-04 and one regression guard for invalid
   metadata inputs.

## Affected Areas

- `crates/tau-trainer/src/lib.rs`
- `crates/tau-trainer/src/benchmark_artifact.rs` (new)
- `specs/1966/spec.md`
- `specs/1966/plan.md`
- `specs/1966/tasks.md`

## Risks And Mitigations

- Risk: non-deterministic payload shape over time.
  - Mitigation: constant schema version and explicit top-level fields.
- Risk: optional reproducibility sections collapse silently.
  - Mitigation: force explicit `null` serialization and conformance tests.

## ADR

No dependency/protocol changes; ADR not required.
