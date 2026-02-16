# Plan #2066

Status: Implemented
Spec: specs/2066/spec.md

## Approach

1. Add deterministic `channel-store-admin-split-map.sh` generator with explicit
   phase ownership, line-reduction estimates, import/API impact, and test
   migration plan.
2. Add schema + report artifacts and guide documentation under the M25
   contract paths.
3. Add shell + Python contract tests to lock artifact requirements and fail
   closed on invalid inputs.
4. Generate baseline reports with deterministic timestamp for reproducible
   planning evidence.

## Affected Modules

- `scripts/dev/channel-store-admin-split-map.sh`
- `scripts/dev/test-channel-store-admin-split-map.sh`
- `tasks/schemas/m25-channel-store-admin-split-map.schema.json`
- `tasks/reports/m25-channel-store-admin-split-map.json`
- `tasks/reports/m25-channel-store-admin-split-map.md`
- `docs/guides/channel-store-admin-split-map.md`
- `.github/scripts/test_channel_store_admin_split_map_contract.py`
- `docs/README.md`

## Risks and Mitigations

- Risk: split map drifts as source file changes between planning and execution.
  - Mitigation: compute current line count from live source on each run.
- Risk: artifact contract omissions break governance reproducibility.
  - Mitigation: enforce schema/guide/report presence and field requirements via
    shell + Python tests.

## Interfaces and Contracts

- Generator:
  `scripts/dev/channel-store-admin-split-map.sh --output-json <path> --output-md <path>`
- Schema:
  `tasks/schemas/m25-channel-store-admin-split-map.schema.json`
- Validation:
  `scripts/dev/test-channel-store-admin-split-map.sh`

## ADR References

- Not required.
