# Tasks #2059

Status: Implemented
Spec: specs/2059/spec.md
Plan: specs/2059/plan.md

## Ordered Tasks

- T1: Update split guardrail checks to reflect `<3000` target and new module
  markers.
- T2: Extract execution-domain flag block into new module and wire flatten field
  in `Cli`.
- T3: Run regression/validation commands (`scripts/dev/test-cli-args-domain-split.sh`,
  python governance suites, scoped Rust checks where available).
- T4: Record evidence and close subtask + parent task if all ACs pass.
