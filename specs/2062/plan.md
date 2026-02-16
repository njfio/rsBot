# Plan #2062

Status: Implemented
Spec: specs/2062/spec.md

## Approach

1. Add a deterministic tools split-map generator and schema.
2. Emit canonical planning artifacts in `tasks/reports/`.
3. Add shell + Python contract tests for artifact shape, guide linkage, and
   fail-closed behavior.
4. Publish guide references for API/import impact and test migration.

## Affected Modules

- `scripts/dev/tools-split-map.sh`
- `scripts/dev/test-tools-split-map.sh`
- `tasks/schemas/m25-tools-split-map.schema.json`
- `tasks/reports/m25-tools-split-map.json`
- `tasks/reports/m25-tools-split-map.md`
- `docs/guides/tools-split-map.md`
- `.github/scripts/test_tools_split_map_contract.py`

## Risks and Mitigations

- Risk: extraction estimates drift as `tools.rs` evolves.
  - Mitigation: compute source line count from live file on each generation.
- Risk: tool entrypoint contracts omitted from split documentation.
  - Mitigation: require non-empty API/import impact sections in contract tests.

## Interfaces and Contracts

- Generator:
  `scripts/dev/tools-split-map.sh --output-json <path> --output-md <path>`
- Schema:
  `tasks/schemas/m25-tools-split-map.schema.json`
- Validation:
  `scripts/dev/test-tools-split-map.sh`

## ADR References

- Not required.
