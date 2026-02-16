# Issue 1980 Plan

Status: Reviewed

## Approach

1. Add summary models in `benchmark_artifact.rs`:
   - `BenchmarkArtifactGateReportSummaryEntry`
   - `BenchmarkArtifactGateReportSummaryInvalidFile`
   - `BenchmarkArtifactGateReportSummaryManifest`
2. Add builder:
   - `build_benchmark_artifact_gate_report_summary_manifest(directory)`
   - scan `.json` files, parse via existing gate-report validator, record
     deterministic valid/invalid entries
3. Add `to_json_value()` for summary manifest with deterministic totals and
   sections.
4. Add tests for C-01..C-04 plus one regression guard.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1980/spec.md`
- `specs/1980/plan.md`
- `specs/1980/tasks.md`

## Risks And Mitigations

- Risk: summary ordering drift across filesystem iteration.
  - Mitigation: collect candidate JSON paths and sort before parsing.
- Risk: parse failures aborting full scan.
  - Mitigation: accumulate invalid diagnostics and continue.

## ADR

No dependency/protocol changes; ADR not required.
