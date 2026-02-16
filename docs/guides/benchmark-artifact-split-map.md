# Benchmark Artifact Split Map (M25)

This guide defines the pre-extraction split plan for:

- `crates/tau-trainer/src/benchmark_artifact.rs`

Goal:

- reduce the primary file below 3000 LOC in `#2041` while preserving benchmark
  artifact behavior and report contracts.

## Generate Artifacts

```bash
scripts/dev/benchmark-artifact-split-map.sh
```

Default outputs:

- `tasks/reports/m25-benchmark-artifact-split-map.json`
- `tasks/reports/m25-benchmark-artifact-split-map.md`

Schema:

- `tasks/schemas/m25-benchmark-artifact-split-map.schema.json`

Deterministic replay:

```bash
scripts/dev/benchmark-artifact-split-map.sh \
  --generated-at 2026-02-16T00:00:00Z \
  --output-json /tmp/m25-benchmark-artifact-split-map.json \
  --output-md /tmp/m25-benchmark-artifact-split-map.md
```

## Validation

```bash
scripts/dev/test-benchmark-artifact-split-map.sh
python3 -m unittest discover -s .github/scripts -p "test_benchmark_artifact_split_map_contract.py"
```

## Public API Impact

- Benchmark artifact struct/serde contracts remain stable during extraction.
- Existing trainer call signatures for load/render/compare paths remain
  unchanged.
- Domain modules become internal organization boundaries behind existing entry
  points.

## Import Impact

- New domain modules are introduced under `crates/tau-trainer/src/benchmark_artifact/`.
- Root `benchmark_artifact.rs` keeps explicit re-exports while phases migrate.
- Cross-module imports are minimized by grouping schema/IO/report/validation
  concerns.

## Test Migration Plan

- Run `cargo test -p tau-trainer benchmark_artifact` after each phase.
- Run `cargo test -p tau-trainer` to confirm integration behavior.
- Run `python3 -m unittest discover -s .github/scripts -p "test_*.py"` to keep
  governance contracts green.
