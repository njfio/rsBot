# Issue 1686 Plan

Status: Reviewed

## Approach

1. Create module directory `crates/tau-memory/src/runtime/`.
2. Move backend-resolution and persistence functions to `backend.rs`.
3. Move ranking/embedding functions to `ranking.rs`.
4. Move search/tree/query-oriented `FileMemoryStore` impl blocks to `query.rs`.
5. Keep `runtime.rs` as API/composition surface and preserve public signatures.
6. Add split harness script to enforce module boundaries.
7. Run scoped verification for `tau-memory`.

## Affected Areas

- `crates/tau-memory/src/runtime.rs`
- `crates/tau-memory/src/runtime/backend.rs`
- `crates/tau-memory/src/runtime/ranking.rs`
- `crates/tau-memory/src/runtime/query.rs`
- `scripts/dev/test-memory-runtime-domain-split.sh`
- `specs/1686/*`

## Risks And Mitigations

- Risk: scope/record normalization behavior drift.
  - Mitigation: move helper implementations verbatim and retain existing
    call-sites.
- Risk: backend reason-code changes from accidental edits.
  - Mitigation: keep reason constants untouched and run existing regression
    tests.
- Risk: split introduces visibility/import churn.
  - Mitigation: use `pub(super)` boundaries and strict clippy gate.

## ADR

No protocol/dependency/architecture policy change; ADR not required.
