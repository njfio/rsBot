# Tasks #2438

Status: In Progress
Spec: specs/2438/spec.md
Plan: specs/2438/plan.md

- T1 (tests first): capture RED for roadmap sync check and missing preflight
  wrapper tests.
- T2: regenerate roadmap status docs to satisfy freshness gate.
- T3: implement `scripts/dev/preflight-fast.sh` wrapper.
- T4: add `scripts/dev/test-preflight-fast.sh` coverage.
- T5: run green validation (`roadmap-status-sync --check`, wrapper test,
  `cargo fmt --check`, scoped clippy/tests via preflight usage example).
