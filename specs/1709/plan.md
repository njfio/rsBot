# Issue 1709 Plan

Status: Reviewed

## Approach

1. Add `scripts/demo/m24-rl-benchmark-significance-report.sh` that:
   - reads baseline/trained sample arrays from JSON files
   - computes deterministic summary + improvement significance metrics
   - writes a `report_kind=significance` benchmark report artifact
2. Reuse benchmark report validator contract
   (`scripts/demo/validate-m24-rl-benchmark-report.sh`) for compatibility gate.
3. Add `scripts/demo/test-m24-rl-benchmark-significance-report.sh` with:
   - positive generation + validator pass path
   - invalid-input fail-closed regression cases
4. Update operator docs with significance-report generation command.

## Affected Areas

- `scripts/demo/m24-rl-benchmark-significance-report.sh` (new)
- `scripts/demo/test-m24-rl-benchmark-significance-report.sh` (new)
- `docs/guides/training-ops.md`
- `docs/README.md`
- `specs/1709/spec.md`
- `specs/1709/plan.md`
- `specs/1709/tasks.md`

## Risks And Mitigations

- Risk: inconsistent statistical formulas between tooling.
  - Mitigation: lock deterministic formula and output fields in script tests.
- Risk: artifact fields drift from validator contract.
  - Mitigation: run validator in functional test path.
- Risk: malformed sample files cause silent bad reports.
  - Mitigation: strict parse/finite/length checks and fail-closed exit codes.

## ADR

No new dependency/protocol/architecture boundary; ADR not required.
