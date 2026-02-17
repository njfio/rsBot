# Tasks #2362

Status: Completed
Spec: specs/2362/spec.md
Plan: specs/2362/plan.md

- T1 (tests first): add failing C-01..C-04 tier-selection/retention tests and
  C-05 pressure-path functional test.
- T2: add `AgentConfig` tier threshold/retention fields with safe defaults.
- T3: implement tiered compaction helpers and request-prep invocation path.
- T4: run scoped verification (`tau-agent-core` tests, `fmt`, `clippy`) and
  capture RED/GREEN evidence for PR.
