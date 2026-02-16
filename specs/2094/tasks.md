# Tasks #2094

Status: Implemented
Spec: specs/2094/spec.md
Plan: specs/2094/plan.md

## Ordered Tasks

- T1 (RED): carry RED evidence from subtask `#2097` pre-extraction split guard failure.
- T2 (GREEN): consume merged hierarchy outputs from PRs `#2099`, `#2100`, and `#2101`.
- T3 (VERIFY): run
  `bash scripts/dev/test-cli-args-domain-split.sh`,
  `cargo check -p tau-cli --lib --target-dir target-fast`,
  and `cargo test -p tau-coding-agent startup_preflight_and_policy --target-dir target-fast`.
- T4 (CLOSE): set epic status done and close issue `#2094` with conformance evidence.
