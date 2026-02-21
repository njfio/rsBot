# Tasks: Issue #3000 - preflight-fast safety guard integration

1. [x] T1 (RED): extend `scripts/dev/test-preflight-fast.sh` with guard-stage expectations and execute to capture failure against current script.
2. [x] T2 (GREEN): add panic/unsafe guard invocation to `scripts/dev/preflight-fast.sh` between roadmap check and `fast-validate`.
3. [x] T3 (REGRESSION): run `scripts/dev/test-preflight-fast.sh`, `scripts/dev/test-panic-unsafe-guard.sh`, and `scripts/dev/test-fast-validate.sh`.
4. [x] T4 (VERIFY): run `cargo fmt --check`.
5. [x] T5 (CONFORMANCE): map and verify C-01..C-05 evidence.

## Tier Mapping
- Unit: N/A (shell tooling only).
- Property: N/A (no randomized invariants).
- Contract/DbC: N/A (no Rust contract macros touched).
- Snapshot: N/A (no snapshot artifacts).
- Functional: preflight-fast success/failure path behavior.
- Conformance: C-01..C-05.
- Integration: script chaining across roadmap sync, panic/unsafe guard, and fast-validate.
- Fuzz: N/A (no parser attack-surface change).
- Mutation: N/A (shell tooling slice).
- Regression: expanded `test-preflight-fast.sh`.
- Performance: N/A (no runtime hotpath change).
