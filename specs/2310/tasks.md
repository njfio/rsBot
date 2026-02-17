# Tasks #2310

Status: Completed
Spec: specs/2310/spec.md
Plan: specs/2310/plan.md

- T1 (tests first): add failing conformance tests C-01..C-03 in `gateway_openresponses` integration suite.
- T2: wire gateway `AgentConfig` preflight token ceilings derived from `max_input_chars`.
- T3: run scoped verification (`cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, `cargo test -p tau-gateway gateway_openresponses`, `cargo test -p tau-gateway`).
- T4: update issue process log and prepare PR with AC mapping + RED/GREEN evidence.
