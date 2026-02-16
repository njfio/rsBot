# Plan #2060

Status: Implemented
Spec: specs/2060/spec.md

## Approach

1. Add a deterministic benchmark split-map generator and schema.
2. Emit canonical planning artifacts in `tasks/reports/`.
3. Add shell + Python contract tests for artifact shape, guide linkage, and
   fail-closed behavior.
4. Publish operator guide references for API/import impact and test migration.

## Affected Modules

- `scripts/dev/benchmark-artifact-split-map.sh`
- `scripts/dev/test-benchmark-artifact-split-map.sh`
- `tasks/schemas/m25-benchmark-artifact-split-map.schema.json`
- `tasks/reports/m25-benchmark-artifact-split-map.json`
- `tasks/reports/m25-benchmark-artifact-split-map.md`
- `docs/guides/benchmark-artifact-split-map.md`
- `.github/scripts/test_benchmark_artifact_split_map_contract.py`

## Risks and Mitigations

- Risk: extraction estimates diverge as source file evolves.
  - Mitigation: generator computes live source line counts each run.
- Risk: split map omits integration-critical interfaces.
  - Mitigation: enforce API/import impact and migration sections as required
    contract fields.

## Interfaces and Contracts

- Generator:
  `scripts/dev/benchmark-artifact-split-map.sh --output-json <path> --output-md <path>`
- Schema:
  `tasks/schemas/m25-benchmark-artifact-split-map.schema.json`
- Validation:
  `scripts/dev/test-benchmark-artifact-split-map.sh`

## ADR References

- Not required.
