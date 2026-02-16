# Issue 1685 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): capture baseline scoped `tau-runtime` behavior via targeted
tests before refactor.

T2: extract parsing/schema validation logic to dedicated parsing module(s).

T3: extract dispatch/lifecycle transition logic to dedicated dispatch module(s).

T4: extract NDJSON transport loops to dedicated transport module(s).

T5: keep root runtime file as composition surface; preserve API and test imports.

T6: add split harness script validating module split shape and key symbol moves.

T7: run scoped verification (`cargo test -p tau-runtime`, strict clippy, fmt,
roadmap sync check), then prepare PR evidence.

## Tier Mapping

- Unit: existing `tau-runtime` unit tests in `rpc_protocol_runtime.rs`
- Functional: split harness assertions over file/module boundaries
- Conformance: AC/C-case mapping verified in test + harness outputs
- Integration: NDJSON dispatch/serve + fixture replay tests
- Regression: crate test suite + strict clippy/fmt checks
