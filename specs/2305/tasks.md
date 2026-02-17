# Tasks #2305

Status: Done
Spec: specs/2305/spec.md
Plan: specs/2305/plan.md

- T1 (tests first): add failing conformance tests C-01 and C-02 in gateway OpenResponses integration tests.
- T2: add gateway session usage persistence helper and wire it into OpenResponses request execution.
- T3: run scoped verification (`cargo fmt --check`, `cargo clippy -p tau-gateway -- -D warnings`, targeted `cargo test -p tau-gateway gateway_openresponses`).
- T4: confirm AC-to-test mapping, update issue process log, and prepare PR evidence (RED/GREEN outputs).
