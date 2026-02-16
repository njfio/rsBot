# Issue 1688 Plan

Status: Reviewed

## Approach

1. Split orchestration code by domain:
   - `load_registry.rs`: catalog loading, install/update, remote fetch/cache, lockfile, registry manifest/source resolution, selection and prompt assembly
   - `trust_policy.rs`: trust root/key-map validation, signature verification, hash/time helpers
2. Keep all public entry points defined in `lib.rs`, delegating to new modules to preserve API.
3. Keep internal test coverage stable by exposing only needed `pub(crate)` helpers for existing tests.
4. Run scoped verification: targeted tests first, then full `tau-skills` test suite, clippy, and fmt.

## Affected Areas

- `crates/tau-skills/src/lib.rs`
- `crates/tau-skills/src/load_registry.rs` (new)
- `crates/tau-skills/src/trust_policy.rs` (new)
- `specs/1688/*`

## Risks And Mitigations

- Risk: subtle behavior drift during function moves.
  - Mitigation: move code verbatim where possible; preserve error text and call order.
- Risk: test visibility breakage for internal helpers.
  - Mitigation: add minimal `pub(crate)` helper exports used by existing tests.
- Risk: clippy dead-code warnings from duplicated leftovers.
  - Mitigation: remove superseded root implementations after delegation.

## ADR

No dependency/protocol/architecture decision beyond internal module decomposition. ADR not required.
