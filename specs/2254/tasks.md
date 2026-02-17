# Tasks #2254

Status: Completed
Spec: specs/2254/spec.md
Plan: specs/2254/plan.md

- T1 (tests first): add failing conformance tests for session usage/cost stats
  output and persistence (C-01, C-02, C-03).
- T2: implement persisted session usage ledger read/write + `SessionStore` delta
  recording.
- T3: extend `SessionStats` compute/render (text + json) with usage fields.
- T4: wire runtime prompt success path to record usage/cost deltas from
  `Agent::cost_snapshot()`.
- T5: run scoped quality gates and update tests to green.
