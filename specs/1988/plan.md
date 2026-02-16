# Issue 1988 Plan

Status: Reviewed

## Approach

1. Add manifest models in `benchmark_artifact.rs`:
   - `BenchmarkArtifactGateSummaryReportManifestEntry`
   - `BenchmarkArtifactGateSummaryReportManifestInvalidFile`
   - `BenchmarkArtifactGateSummaryReportManifest`
2. Add builder:
   - `build_benchmark_artifact_gate_summary_report_manifest(directory)`
   - scan `.json` files, parse via existing summary gate report validator,
     record deterministic valid/invalid entries
3. Add `to_json_value()` for manifest with deterministic totals/sections.
4. Add tests for C-01..C-04 plus non-json-file regression.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1988/spec.md`
- `specs/1988/plan.md`
- `specs/1988/tasks.md`

## Risks And Mitigations

- Risk: unstable ordering from filesystem iteration.
  - Mitigation: collect and sort candidate JSON paths before parse.
- Risk: parse failures aborting the entire scan.
  - Mitigation: accumulate invalid diagnostics and continue scanning.

## ADR

No dependency/protocol changes; ADR not required.
