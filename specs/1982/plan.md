# Issue 1982 Plan

Status: Reviewed

## Approach

1. Add summary quality models in `benchmark_artifact.rs`:
   - `BenchmarkArtifactGateReportSummaryQualityPolicy`
   - `BenchmarkArtifactGateReportSummaryQualityDecision`
2. Add evaluator helper:
   - `evaluate_benchmark_gate_report_summary_quality(summary, policy)`
   - derive total entries and ratios from summary counters
   - emit deterministic reason codes
3. Add `to_json_value()` for quality decision.
4. Add tests for C-01..C-04 plus a zero-entry regression guard.

## Affected Areas

- `crates/tau-trainer/src/benchmark_artifact.rs`
- `specs/1982/spec.md`
- `specs/1982/plan.md`
- `specs/1982/tasks.md`

## Risks And Mitigations

- Risk: ratio math drift for zero-entry summaries.
  - Mitigation: explicit zero-denominator handling (`0.0`) and regression test.
- Risk: ambiguous reason ordering.
  - Mitigation: deterministic append order for reason codes.

## ADR

No dependency/protocol changes; ADR not required.
