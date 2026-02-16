# Issue 1685 Plan

Status: Reviewed

## Approach

1. Create module directory `crates/tau-runtime/src/rpc_protocol_runtime/`.
2. Move parser/schema validation logic into `parsing.rs`.
3. Move frame dispatch + lifecycle state transition logic into `dispatch.rs`.
4. Move NDJSON dispatch/serve transport loops into `transport.rs`.
5. Keep `rpc_protocol_runtime.rs` as public composition layer:
   - constants/types/public API remain stable
   - delegate to extracted module functions
6. Add split harness script to enforce module split shape.
7. Run scoped verification gates (`tau-runtime` tests, strict clippy, fmt,
   roadmap sync check).

## Affected Areas

- `crates/tau-runtime/src/rpc_protocol_runtime.rs`
- `crates/tau-runtime/src/rpc_protocol_runtime/parsing.rs`
- `crates/tau-runtime/src/rpc_protocol_runtime/dispatch.rs`
- `crates/tau-runtime/src/rpc_protocol_runtime/transport.rs`
- `scripts/dev/test-rpc-protocol-runtime-domain-split.sh`
- `specs/1685/*`

## Risks And Mitigations

- Risk: accidental error-message drift can break code-contract fixture tests.
  - Mitigation: move existing strings and helper logic verbatim; run fixture
    replay tests in `tau-runtime`.
- Risk: visibility/import regressions after splitting private helpers.
  - Mitigation: use `pub(super)` boundaries and preserve existing root function
    names used by tests.
- Risk: transport response ordering changes in serve mode.
  - Mitigation: keep transport loops unchanged except delegation boundaries; run
    existing integration/regression tests.

## ADR

No dependency, protocol, or architecture policy change. ADR not required.
