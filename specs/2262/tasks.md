# Tasks #2262

Status: Completed
Spec: specs/2262/spec.md
Plan: specs/2262/plan.md

- T1 (tests first): completed deployment conformance tests for preview2 ABI
  required field and matcher behavior (RED if needed).
- T2: completed deployment runtime constraint defaults migration from preview1 to preview2
  ABI pattern and implement wildcard matcher support.
- T3: completed deployment runbook update for preview2 ABI posture.
- T4: completed validation:
  - `cargo fmt --check`
  - `cargo test -p tau-deployment`
  - `./scripts/dev/wasm-smoke.sh`
- T5: finalize lifecycle artifacts, open PR, merge, close issue with done status.
