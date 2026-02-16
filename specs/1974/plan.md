# Issue 1974 Plan

Status: Reviewed

## Approach

1. Add manifest quality models in `benchmark_artifact.rs`:
   - `BenchmarkArtifactManifestQualityPolicy`
   - `BenchmarkArtifactManifestQualityDecision`
2. Add `evaluate_benchmark_manifest_quality(manifest, policy)` helper:
   - compute valid/invalid counts and invalid ratio
   - compare against policy thresholds
   - emit deterministic reason codes
3. Add decision JSON projection helper.
4. Add conformance tests C-01..C-04 plus regression guardrails.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1974/spec.md`
- `specs/1974/plan.md`
- `specs/1974/tasks.md`

## Risks And Mitigations

- Risk: ambiguous policy edge-case behavior when scanned count is zero.
  - Mitigation: deterministic ratio fallback and explicit no-valid reason code.
- Risk: non-deterministic reason ordering.
  - Mitigation: append reason codes in fixed policy-check order.

## ADR

No dependency/protocol changes; ADR not required.
