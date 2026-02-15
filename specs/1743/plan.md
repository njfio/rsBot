# Issue 1743 Plan

Status: Reviewed

## Approach

1. Add benchmark report publication template JSON under `scripts/demo/`.
2. Add validator script that enforces:
   - report schema fields
   - archival file naming/path conventions
   - retention metadata fields and ranges
3. Add regression test script with valid and invalid fixtures.
4. Update `docs/guides/training-ops.md` with publication format and archival
   policy.

## Affected Areas

- `docs/guides/training-ops.md`
- `scripts/demo/m24-rl-benchmark-report-template.json`
- `scripts/demo/validate-m24-rl-benchmark-report.sh`
- `scripts/demo/test-m24-rl-benchmark-report.sh`
- `specs/1743/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: over-constraining run IDs too early.
  - Mitigation: enforce stable but permissive run-id pattern.
- Risk: retention policy drift.
  - Mitigation: encode retention fields in validator and regression tests.

## ADR

No architecture/dependency/protocol wire-format change. ADR not required.
