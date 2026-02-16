# Issue 1688 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): run a focused `tau-skills` test selection touching load/registry/trust flows to establish baseline RED/GREEN evidence anchor.

T2: create `load_registry.rs` and move load/registry/cache/lockfile orchestration and helper internals.

T3: create `trust_policy.rs` and move trust/signature/hash/time orchestration and helper internals.

T4: refactor `lib.rs` into composition/delegation surface, preserving public API and required `pub(crate)` helpers.

T5: run verification (`cargo test -p tau-skills`, strict clippy for crate, `cargo fmt --check`) and capture evidence.

## Tier Mapping

- Unit: existing unit tests in `tau-skills` remain green
- Functional: module split conformance (`lib.rs` delegates)
- Integration: trust + registry + remote/cache flows
- Regression: full crate tests + lint/format checks
