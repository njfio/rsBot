# Issue 1972 Plan

Status: Reviewed

## Approach

1. Add manifest models in `benchmark_artifact.rs`:
   - `BenchmarkArtifactManifestEntry`
   - `BenchmarkArtifactInvalidFile`
   - `BenchmarkArtifactManifest`
2. Add `build_benchmark_artifact_manifest(directory)` helper:
   - ensure directory exists and is a directory
   - scan `*.json` files and sort paths deterministically
   - parse artifact payloads and collect key metadata for valid files
   - capture invalid file diagnostics without aborting scan
3. Add `BenchmarkArtifactManifest::to_json_value()`.
4. Add conformance tests C-01..C-04 plus regression coverage.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1972/spec.md`
- `specs/1972/plan.md`
- `specs/1972/tasks.md`

## Risks And Mitigations

- Risk: non-deterministic filesystem scan order.
  - Mitigation: canonicalized/sorted path list before processing.
- Risk: malformed artifacts terminate whole scan.
  - Mitigation: capture per-file errors in diagnostics and continue scanning.

## ADR

No dependency/protocol changes; ADR not required.
