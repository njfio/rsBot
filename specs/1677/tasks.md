# Issue 1677 Tasks

Status: Reviewed

## Ordered Tasks

T1 (tests-first): add failing tests for crash-detected resume recovery report,
checkpoint primary/fallback restore behavior, and missing-checkpoint guardrail
failure.

T2: implement resume recovery workflow and report persistence in
`training_runtime`.

T3: wire deterministic checkpoint discovery (`policy-checkpoint.json` /
`policy-checkpoint.rollback.json`) and replay metadata extraction from control
audit log.

T4: add operator runbook documentation for recovery execution and failure paths.

T5: run scoped verification and map AC-1..AC-4 to C-01..C-05 evidence.

## Tier Mapping

- Unit: recovery helper/state classification behavior
- Functional: resume recovery report + primary restore behavior
- Integration: interrupted-state resume end-to-end
- Regression: corrupted/missing checkpoint guardrail paths
- Conformance: C-01..C-05
