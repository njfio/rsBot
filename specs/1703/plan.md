# Issue 1703 Plan

Status: Reviewed

## Approach

1. Add/adjust matrix script contract tests to require repository-relative artifact
   paths in JSON and Markdown outputs.
2. Update `scripts/dev/m21-validation-matrix.sh` path emission logic.
3. Regenerate `tasks/reports/m21-validation-matrix.json` and
   `tasks/reports/m21-validation-matrix.md` from live M21 state.
4. Publish gate evidence comment on `#1703` and close issue when complete.

## Affected Areas

- `scripts/dev/m21-validation-matrix.sh`
- `scripts/dev/test-m21-validation-matrix.sh`
- `tasks/reports/m21-validation-matrix.json`
- `tasks/reports/m21-validation-matrix.md`

## Risks And Mitigations

- Risk: matrix summary values change quickly due issue churn
  - Mitigation: include generation timestamp and command in gate comment.
- Risk: relative-path conversion could break fixture expectations
  - Mitigation: tests-first updates on both JSON and Markdown path assertions.

## ADR

No new dependency/protocol changes; ADR not required.
