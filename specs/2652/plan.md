# Plan: Issue #2652 - SecretStore contract, DecryptedSecret redaction, and machine-auto encryption (G20 phase 1)

## Approach
1. Add RED tests for `DecryptedSecret` redaction, `SecretStore` trait-backed roundtrip, and auto-mode keyed behavior without explicit passphrase.
2. Introduce `DecryptedSecret` wrapper and `SecretStore` trait with a file-backed implementation (`FileSecretStore`) that delegates to existing credential-store load/save functions.
3. Update credential-store key derivation to support machine-derived fallback when keyed mode is selected with no passphrase.
4. Update `resolve_credential_store_encryption_mode` auto branch to always select keyed encryption.
5. Run scoped verification and keep integration compatibility tests green.

## Affected Modules
- `crates/tau-provider/src/credential_store.rs`
- `crates/tau-provider/src/lib.rs` (export surface, if needed)
- `crates/tau-coding-agent/src/tests/auth_provider/auth_and_provider/provider_client_and_store.rs` (auto mode expectation)
- `specs/milestones/m107/index.md`
- `specs/2652/spec.md`
- `specs/2652/plan.md`
- `specs/2652/tasks.md`

## Risks / Mitigations
- Risk: machine-derived fallback could become unstable across environments.
  - Mitigation: derive from stable host/user env markers with deterministic hashing and test roundtrip in one runtime.
- Risk: changing auto mode could break existing assumptions in tests/flows.
  - Mitigation: add explicit regression coverage for auto behavior and keep explicit `none` mode unchanged.
- Risk: introducing trait abstraction without usage can drift.
  - Mitigation: add roundtrip tests directly through trait implementation and wire at least one runtime helper through it.

## Interfaces / Contracts
- New public `DecryptedSecret` wrapper for plaintext handling with redacted formatting.
- New public `SecretStore` trait and `FileSecretStore` implementation in `tau-provider`.
- Existing credential-store schema and `load_credential_store`/`save_credential_store` interfaces remain stable.

## ADR
- Not required for phase-1 incremental hardening (no new dependency or schema/protocol decision).
