# Tasks #2259

Status: Done
Spec: specs/2259/spec.md
Plan: specs/2259/plan.md

- T1 (tests first): add failing conformance tests C-01..C-05 for Postgres backend
  selection, entry round-trip, usage summary round-trip, isolation, and DSN failure
  behavior.
- T2: add PostgreSQL backend implementation for session entry read/write and schema
  initialization.
- T3: add PostgreSQL backend implementation for usage summary read/write.
- T4: run scoped verification (`fmt`, `clippy`, `cargo test -p tau-session`) and
  confirm AC-to-test mapping.
- T5: update issue process log, open PR with Red/Green evidence, and set
  `specs/2259/spec.md` status to `Implemented` after merge.
