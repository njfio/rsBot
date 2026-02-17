# Tasks #2260

Status: Completed
Spec: specs/2260/spec.md
Plan: specs/2260/plan.md

- T1 (tests first): add failing deterministic command-flow tests for interactive
  execution, workspace-root persistence, cancel semantics, and non-interactive
  regression (C-01..C-04).
- T2: refactor onboarding command execution to inject prompt/output adapters while
  preserving public API.
- T3: apply wizard-selected workspace root to onboarding persistence/report path
  resolution.
- T4: run scoped verification (`fmt`, `clippy`, `cargo test -p tau-onboarding`) and
  map ACs to passing tests.
- T5: update issue process log and open PR with Red/Green evidence.
