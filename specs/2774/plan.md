# Plan: Issue #2774 - G20 crypto/store dependency closure and AES-GCM hardening

## Approach
1. Add RED tests covering AES-GCM envelope behavior, legacy decrypt compatibility, and tamper failure.
2. Add workspace dependency declarations for `aes-gcm` and `redb`.
3. Harden `tau-provider::credential_store` keyed encryption to AES-256-GCM for new writes.
4. Preserve legacy `enc:v1:` decrypt path for backward compatibility.
5. Run scoped verification and roadmap/spec updates.

## Affected Modules
- `Cargo.toml`
- `Cargo.lock`
- `crates/tau-provider/Cargo.toml`
- `crates/tau-provider/src/credential_store.rs`
- `tasks/spacebot-comparison.md`
- `specs/2774/*`

## Risks and Mitigations
- Risk: breaking existing encrypted credential stores.
  - Mitigation: explicit legacy decrypt support + regression conformance test.
- Risk: auth failures from malformed payload parsing.
  - Mitigation: fail-closed error handling with deterministic reason strings.
- Risk: dependency bloat.
  - Mitigation: keep `redb` declaration scoped for roadmap closure; do not migrate backend in this slice.

## Interface and Contract Notes
- Public API signatures remain unchanged.
- Keyed payload envelope evolves to `enc:v2:` for new writes.
- Decrypt path supports both `enc:v2:` and legacy `enc:v1:`.
