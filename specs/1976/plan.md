# Issue 1976 Plan

Status: Reviewed

## Approach

1. Add report model(s) in `benchmark_artifact.rs`:
   - `BenchmarkArtifactGateReport`
2. Add builder:
   - `build_benchmark_artifact_gate_report(manifest, policy)`
   - evaluate quality decision via existing
     `evaluate_benchmark_manifest_quality(...)`
   - return combined report
3. Add `to_json_value()` with nested `manifest` and `quality` sections.
4. Add tests for C-01..C-04 and one regression guard.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1976/spec.md`
- `specs/1976/plan.md`
- `specs/1976/tasks.md`

## Risks And Mitigations

- Risk: drift between manifest counters and quality input mapping.
  - Mitigation: quality input derived directly from manifest lengths.
- Risk: unstable payload shape.
  - Mitigation: fixed top-level keys + explicit nested sections.

## ADR

No dependency/protocol changes; ADR not required.
