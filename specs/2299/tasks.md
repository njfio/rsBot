# Tasks #2299

Status: In Progress
Spec: specs/2299/spec.md
Plan: specs/2299/plan.md

- T1 (tests first): add failing conformance tests C-01..C-05 for OpenRouter payload mapping, merge precedence, and cache fallback behavior.
- T2: implement OpenRouter payload mapping in `parse_model_catalog_payload`.
- T3: implement deterministic catalog merge helper and apply it in remote refresh path.
- T4: run scoped verification (`cargo fmt --check`, `cargo clippy -p tau-provider -- -D warnings`, targeted `cargo test -p tau-provider model_catalog`).
- T5: update issue process log and prepare PR with AC mapping + RED/GREEN evidence.
