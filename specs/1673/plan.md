# Issue 1673 Plan

Status: Reviewed

## Approach

1. Add a `benchmark_driver` module in `tau-trainer` with:
   - benchmark scorer trait
   - suite execution function producing deterministic observation reports
   - repeatability evaluator for cross-run deltas and variance bands
2. Integrate driver with `benchmark_fixtures` types added for `#1697`.
3. Add tests-first conformance coverage for:
   - deterministic repeated runs
   - repeatability tolerance behavior
   - malformed fixture failure-path handling
4. Run scoped verification and map AC/C-xx results in PR.

## Affected Areas

- `crates/tau-trainer/src/lib.rs`
- `crates/tau-trainer/src/benchmark_driver.rs` (new)
- `specs/1673/spec.md`
- `specs/1673/plan.md`
- `specs/1673/tasks.md`

## Risks And Mitigations

- Risk: repeatability logic can hide case-level drift.
  - Mitigation: include per-case range/max-delta fields in report.
- Risk: scorer trait may be too narrow for future runtime usage.
  - Mitigation: keep trait minimal and deterministic; expand in follow-up if
    runtime integration needs additional context.

## ADR

No architecture/dependency/protocol change; ADR not required.
