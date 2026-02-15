# Issue 1968 Plan

Status: Reviewed

## Approach

1. Add `BenchmarkArtifactExportSummary` in `benchmark_artifact.rs`.
2. Add `export_benchmark_evaluation_artifact` helper that:
   - validates destination path is directory-like
   - creates destination directory tree when missing
   - writes deterministic filename:
     `benchmark-<suite>-<baseline>-vs-<candidate>-<ts>.json`
   - serializes via `artifact.to_json_value()` as pretty JSON
   - returns written path and byte count
3. Add tests C-01..C-04 using `tempfile::tempdir`.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1968/spec.md`
- `specs/1968/plan.md`
- `specs/1968/tasks.md`

## Risks And Mitigations

- Risk: non-portable file names due unexpected characters.
  - Mitigation: sanitize ids into safe slug segments.
- Risk: directory/file ambiguity at destination path.
  - Mitigation: explicit pre-flight filesystem checks with deterministic errors.

## ADR

No dependency/protocol changes; ADR not required.
