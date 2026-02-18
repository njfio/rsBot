# Tasks #2433

Status: In Progress
Spec: specs/2433/spec.md
Plan: specs/2433/plan.md

- T1 (tests first): run conformance RED for C-01..C-05 and record baseline
  failures.
- T2: patch bash policy metadata signaling for success and fail-closed paths.
- T3: patch bash rate-limit enforcement/state handling for same-principal deny,
  per-principal isolation, and window reset behavior.
- T4: run conformance GREEN for C-01..C-05.
- T5: run `cargo fmt --check` and
  `cargo clippy -p tau-tools --no-deps -- -D warnings`.
