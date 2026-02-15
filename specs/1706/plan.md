# Issue 1706 Plan

Status: Reviewed

## Approach

1. Add `scripts/dev/m22-compatibility-alias-validation.sh` to run targeted
   alias compatibility tests and emit JSON + Markdown artifacts.
2. Add `scripts/dev/test-m22-compatibility-alias-validation.sh` contract tests
   for script functional/regression behavior.
3. Update training operations docs with final compatibility policy and canonical
   migration guidance.
4. Update docs index for policy discoverability.

## Affected Areas

- `scripts/dev/m22-compatibility-alias-validation.sh` (new)
- `scripts/dev/test-m22-compatibility-alias-validation.sh` (new)
- `docs/guides/training-ops.md` (updated)
- `docs/README.md` (updated)
- `tasks/reports/m22-compatibility-alias-validation.{json,md}` (generated)

## Output Contracts

JSON report minimum fields:

- `schema_version`
- `generated_at`
- `repo_root`
- `commands[]` (`name`, `cmd`, `status`, `stdout_excerpt`)
- `summary` (`total`, `passed`, `failed`)

Markdown report minimum sections:

- summary
- executed command matrix
- migration policy reminder

## Risks And Mitigations

- Risk: validation script drifts from test names
  - Mitigation: script contract test validates expected command identifiers.
- Risk: policy docs drift
  - Mitigation: docs update in same PR + docs link checks.

## ADR

No dependency/protocol changes. ADR not required.
