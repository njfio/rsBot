# Issue 1686 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add/run split harness in Red mode before refactor.

T2: extract backend selection/persistence logic to `runtime/backend.rs`.

T3: extract ranking/embedding logic to `runtime/ranking.rs`.

T4: extract search/tree/query orchestration impl methods to `runtime/query.rs`.

T5: keep root runtime API stable and verify tests compile unchanged.

T6: run scoped verification (`cargo test -p tau-memory`, strict clippy, fmt,
roadmap sync check) and prepare PR evidence.

## Tier Mapping

- Unit: existing `tau-memory` unit tests
- Functional: split harness module-layout assertions
- Conformance: AC/C-case mapping validated by harness + tests
- Integration: memory store search/tree and backend migration tests
- Regression: crate tests + strict clippy/fmt
