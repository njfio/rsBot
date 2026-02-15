# Issue 1704 Plan

Status: Reviewed

## Approach

1. Add tests-first regression assertion to proof summary script test ensuring
   emitted report/log/artifact paths are relative.
2. Update `scripts/dev/m21-retained-capability-proof-summary.sh` path emission
   to convert output references to repo-relative paths.
3. Run proof summary script in live mode and generate
   `tasks/reports/m21-retained-capability-proof-summary.{json,md}`.
4. Publish gate evidence and troubleshooting notes in issue #1704.

## Affected Areas

- `scripts/dev/test-m21-retained-capability-proof-summary.sh`
- `scripts/dev/m21-retained-capability-proof-summary.sh`
- `tasks/reports/m21-retained-capability-proof-summary.json`
- `tasks/reports/m21-retained-capability-proof-summary.md`

## Risks And Mitigations

- Risk: live matrix run can fail due environment/runtime drift
  - Mitigation: capture run-level diagnostics and include troubleshooting notes.
- Risk: artifact paths remain environment-specific
  - Mitigation: regression assertion for relative-path output.

## ADR

No new dependencies/protocols; ADR not required.
