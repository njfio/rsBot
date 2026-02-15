# Issue 1737 Plan

Status: Reviewed

## Approach

1. Add a calibration module in `tau-algorithm` with:
   - coefficient candidate observation types
   - calibration policy thresholds
   - deterministic candidate ranking + default selection
2. Add a benchmark calibration fixture under `tau-algorithm` testdata.
3. Add tests:
   - functional deterministic ranking
   - integration fixture-based default selection
   - regression fail-closed when no candidate passes

## Affected Areas

- `crates/tau-algorithm/src/lib.rs`
- `crates/tau-algorithm/src/safety_penalty_calibration.rs`
- `crates/tau-algorithm/testdata/safety_penalty_calibration_grid.json`
- `specs/1737/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: ambiguous tie-breaking between candidates.
  - Mitigation: deterministic ordering by reward desc, safety asc, coefficient asc.
- Risk: accidental unsafe default if thresholds are omitted.
  - Mitigation: policy defaults and fail-closed behavior on empty compliant set.

## ADR

No architecture/dependency/protocol change. ADR not required.
