# Tasks #2357

Status: In Progress
Spec: specs/2357/spec.md
Plan: specs/2357/plan.md

- T1 (tests first): add failing C-01..C-04 tests for coalescing helper/runtime behavior.
- T2: implement coalescing helper and source-key processed tracking in runtime loop.
- T3: wire `coalescing_window_ms` through CLI, validation, and runtime config builders.
- T4: add C-05 live runner integration test proving coalesced response behavior.
- T5: run GREEN verification (`cargo test -p tau-multi-channel`, targeted
  `tau-cli` tests, `cargo fmt --check`) and capture evidence.
