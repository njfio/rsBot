# Plan #2058

Status: Implemented
Spec: specs/2058/spec.md

## Approach

Publish a deterministic split-map artifact pipeline for `cli_args.rs`:

1. Add schema + generator script that computes current LOC and records staged
   extraction boundaries/owners.
2. Emit JSON + Markdown artifacts under `tasks/reports/`.
3. Add tests that validate output shape/content and fail closed for invalid
   inputs.
4. Document import/public API impact and test migration plan in the operator
   guide.

## Affected Modules

- `scripts/dev/cli-args-split-map.sh`
- `scripts/dev/test-cli-args-split-map.sh`
- `tasks/schemas/m25-cli-args-split-map.schema.json`
- `docs/guides/cli-args-split-map.md`
- `tasks/reports/m25-cli-args-split-map.json`
- `tasks/reports/m25-cli-args-split-map.md`

## Risks and Mitigations

- Risk: split map becomes stale as `cli_args.rs` evolves.
  - Mitigation: generator computes live line counts and can be rerun
    deterministically.
- Risk: extraction estimates under-shoot needed LOC reduction.
  - Mitigation: include cumulative estimated reduction and explicit remaining
    gap to target.

## Interfaces and Contracts

- Generator command:
  `scripts/dev/cli-args-split-map.sh --output-json <path> --output-md <path>`.
- Schema contract:
  `tasks/schemas/m25-cli-args-split-map.schema.json`.
- Validation command:
  `scripts/dev/test-cli-args-split-map.sh`.

## ADR References

- Not required.
