# Issue 1986 Plan

Status: Reviewed

## Approach

1. Add summary gate report export helper in `benchmark_artifact.rs`:
   - `export_benchmark_artifact_gate_summary_report(report, output_dir)`
   - deterministic filename from stable summary counters
2. Add replay validator helper:
   - `validate_exported_benchmark_artifact_gate_summary_report(path)`
   - enforce top-level object with required `summary` and `quality` keys
3. Add tests for C-01..C-04 and one missing-section regression guard.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1986/spec.md`
- `specs/1986/plan.md`
- `specs/1986/tasks.md`

## Risks And Mitigations

- Risk: payload drift between in-memory report and exported JSON.
  - Mitigation: export serializes `report.to_json_value()` directly.
- Risk: non-deterministic filenames breaking references.
  - Mitigation: deterministic filename from summary counters.

## ADR

No dependency/protocol changes; ADR not required.
