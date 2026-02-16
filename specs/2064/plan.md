# Plan #2064

Status: Implemented
Spec: specs/2064/spec.md

## Approach

1. Add a deterministic GitHub Issues runtime split-map generator and schema.
2. Emit canonical planning artifacts in `tasks/reports/`.
3. Add shell + Python contract tests for artifact shape, guide linkage, and
   fail-closed behavior.
4. Publish guide references for API/import impact and test migration.

## Affected Modules

- `scripts/dev/github-issues-runtime-split-map.sh`
- `scripts/dev/test-github-issues-runtime-split-map.sh`
- `tasks/schemas/m25-github-issues-runtime-split-map.schema.json`
- `tasks/reports/m25-github-issues-runtime-split-map.json`
- `tasks/reports/m25-github-issues-runtime-split-map.md`
- `docs/guides/github-issues-runtime-split-map.md`
- `.github/scripts/test_github_issues_runtime_split_map_contract.py`

## Risks and Mitigations

- Risk: extraction estimates drift as runtime file evolves.
  - Mitigation: compute source line count from live file each run.
- Risk: split map omits bridge-critical public entrypoints.
  - Mitigation: enforce non-empty API/import impact sections via contract tests.

## Interfaces and Contracts

- Generator:
  `scripts/dev/github-issues-runtime-split-map.sh --output-json <path> --output-md <path>`
- Schema:
  `tasks/schemas/m25-github-issues-runtime-split-map.schema.json`
- Validation:
  `scripts/dev/test-github-issues-runtime-split-map.sh`

## ADR References

- Not required.
