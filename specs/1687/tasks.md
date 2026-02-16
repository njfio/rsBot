# Issue 1687 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add split harness and capture Red result before extraction.

T2: extract ingress helper domain to `runtime/ingress.rs`.

T3: extract routing helper domain to `runtime/routing.rs`.

T4: extract outbound/retry helper domain to `runtime/outbound.rs`.

T5: keep root runtime composition surface stable and preserve tests/imports.

T6: run scoped verification (`cargo test -p tau-multi-channel`, strict clippy,
fmt, roadmap sync check) and prepare PR evidence.

## Tier Mapping

- Unit: existing `tau-multi-channel` unit tests
- Functional: split harness module-layout assertions
- Conformance: AC/C-case mapping validated by harness + tests
- Integration: existing live/runtime integration tests
- Regression: crate tests + strict clippy/fmt
