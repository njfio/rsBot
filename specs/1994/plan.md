# Issue 1994 Plan

Status: Reviewed

## Approach

1. Add export helper in `benchmark_artifact.rs`:
   - `export_benchmark_artifact_gate_summary_report_manifest_report(report, output_dir)`
   - deterministic filename from stable manifest counters
2. Add replay validator helper:
   - `validate_exported_benchmark_artifact_gate_summary_report_manifest_report(path)`
   - require top-level object with `manifest` and `quality` keys
3. Add tests for C-01..C-04 and one missing-section regression guard.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1994/spec.md`
- `specs/1994/plan.md`
- `specs/1994/tasks.md`

## Risks And Mitigations

- Risk: payload drift between in-memory report and exported JSON.
  - Mitigation: export serializes `report.to_json_value()` directly.
- Risk: non-deterministic filenames breaking references.
  - Mitigation: deterministic filename from report counters.

## ADR

No dependency/protocol changes; ADR not required.
