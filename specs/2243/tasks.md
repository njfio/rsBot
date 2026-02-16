# Tasks #2243

Status: Implemented
Spec: specs/2243/spec.md
Plan: specs/2243/plan.md

- T1 (RED): Extend deterministic harness tests to include new case IDs and fail
  until harness supports them.
- T2 (GREEN): Implement case catalog + checks in
  `scripts/dev/live-capability-matrix.sh` for AC-1/2/3/4/6/7.
- T3 (GREEN): Add/adjust validators for long-output, stream-mode runs,
  session continuity, and multi-tool call thresholds.
- T4 (GREEN): Add wrapper validator script for AC-1..AC-8 execution and summary.
- T5 (VERIFY): Run deterministic harness tests and scoped `tau-ai` retry tests.
- T6 (VERIFY): Run live provider cases for AC-1/2/3/4/6/7 and confirm pass.
- T7 (DOC/TRACE): Update milestone/issue status and summarize AC-to-test mapping.
