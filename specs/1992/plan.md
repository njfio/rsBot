# Issue 1992 Plan

Status: Reviewed

## Approach

1. Add report model(s) in `benchmark_artifact.rs`:
   - `BenchmarkArtifactGateSummaryReportManifestReport`
2. Add builder helper:
   - `build_benchmark_artifact_gate_summary_report_manifest_report(manifest, policy)`
   - evaluate quality via existing
     `evaluate_benchmark_gate_summary_report_manifest_quality(...)`
   - return combined report
3. Add `to_json_value()` with nested `manifest` and `quality` sections.
4. Add tests for C-01..C-04 and one zero-manifest regression guard.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1992/spec.md`
- `specs/1992/plan.md`
- `specs/1992/tasks.md`

## Risks And Mitigations

- Risk: drift between manifest counters and evaluated quality fields.
  - Mitigation: quality evaluation directly consumes manifest object.
- Risk: unstable payload shape.
  - Mitigation: fixed top-level keys with explicit nested sections.

## ADR

No dependency/protocol changes; ADR not required.
