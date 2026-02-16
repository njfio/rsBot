# Tasks #2096

Status: Implemented
Spec: specs/2096/spec.md
Plan: specs/2096/plan.md

## Ordered Tasks

- T1 (RED): carry forward RED evidence from subtask `#2097`
  (`test-cli-args-domain-split` failed before module wiring).
- T2 (GREEN): consume merged implementation from PR `#2099`
  (execution-domain module extraction + wiring + callsite updates).
- T3 (VERIFY): run
  `bash scripts/dev/test-cli-args-domain-split.sh`,
  `cargo check -p tau-cli --lib --target-dir target-fast`,
  and `cargo test -p tau-coding-agent startup_preflight_and_policy --target-dir target-fast`.
- T4 (CLOSE): set task status done and close issue `#2096` with conformance evidence.
