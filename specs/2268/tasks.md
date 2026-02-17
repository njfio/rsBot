# Tasks #2268

Status: Planned
Spec: specs/2268/spec.md
Plan: specs/2268/plan.md

- T1 (tests first): add RED conformance tests in `tau-core` for C-01..C-03
  (threshold rotation, retention pruning, env policy parsing fallbacks).
- T2: implement shared log rotation policy + append helper in `tau-core`.
- T3: integrate helper into `tau-runtime` appenders (heartbeat, background jobs,
  tool audit, prompt telemetry) and add/adjust runtime tests for C-04.
- T4: integrate helper into transport runtime cycle appenders
  (dashboard/gateway/deployment/multi-agent/multi-channel/custom-command/voice).
- T5: add operator docs updates for C-05 (env controls, defaults, file naming).
- T6: run scoped verification and capture RED/GREEN evidence:
  - `cargo fmt --check`
  - `cargo clippy -p tau-core -p tau-runtime -- -D warnings`
  - `cargo test -p tau-core`
  - `cargo test -p tau-runtime`
  - targeted tests for touched transport crates.
- T7: update issue process log + labels (`status:implementing`), open PR, merge,
  then set `spec.md` to Implemented and `tasks.md` to Completed.
