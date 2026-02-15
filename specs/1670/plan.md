# Issue 1670 Plan

Status: Reviewed

## Approach

1. Add `checkpoint_store` module in `tau-trainer` defining:
   - `PolicyCheckpoint` payload shape
   - checkpoint source/result types for resume diagnostics
   - public save/load APIs and rollback-aware resume API
2. Implement deterministic JSON checkpoint schema with explicit version guard.
3. Implement atomic-ish file save flow (temp write + replace).
4. Add tests-first conformance coverage for:
   - roundtrip integrity
   - fallback resume on corruption
   - unsupported-version rejection
5. Run scoped verification and map AC/C-xx evidence in PR.

## Affected Areas

- `crates/tau-trainer/src/checkpoint_store.rs` (new)
- `crates/tau-trainer/src/lib.rs`
- `specs/1670/spec.md`
- `specs/1670/plan.md`
- `specs/1670/tasks.md`

## Risks And Mitigations

- Risk: partial writes could leave invalid checkpoint files.
  - Mitigation: write temp file then replace destination.
- Risk: fallback silently masking primary corruption.
  - Mitigation: return diagnostics vector with explicit primary-load failure.

## ADR

No new dependency or wire/protocol changes; ADR not required.
