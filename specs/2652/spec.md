# Spec: Issue #2652 - SecretStore contract, DecryptedSecret redaction, and machine-auto encryption (G20 phase 1)

Status: Implemented

## Problem Statement
Tau credential store support currently provides plaintext and keyed encoding modes, but lacks a first-class secret-store contract and a redacted decrypted-secret wrapper for safe logging. In addition, auto encryption mode currently degrades to plaintext when no passphrase is set. G20 phase 1 requires reusable secret-store abstractions plus safer default behavior where auto mode uses machine-derived at-rest encryption.

## Acceptance Criteria

### AC-1 Decrypted secrets are redacted in logs and formatting output
Given a decrypted secret value is wrapped for runtime handling,
When the wrapper is rendered via `Debug` or `Display`,
Then the output is exactly `"[REDACTED]"` and does not leak plaintext.

### AC-2 SecretStore trait exposes reusable file-backed secret persistence
Given a credential store path,
When callers read/write integration secrets through the new `SecretStore` contract,
Then secret roundtrip behavior is preserved using existing credential-store schema and encryption semantics.

### AC-3 Auto encryption mode defaults to keyed machine-derived protection
Given `CliCredentialStoreEncryptionMode::Auto` and no explicit `--credential-store-key`,
When encryption mode is resolved and secrets are persisted,
Then the store uses keyed encryption with machine-derived key material and secrets are not stored as plaintext payloads.

### AC-4 Explicit passphrase keyed mode and existing file compatibility remain intact
Given keyed records encrypted with a provided passphrase and existing plaintext records,
When load/decrypt operations run,
Then passphrase validation and legacy compatibility behavior remain unchanged.

### AC-5 Scoped verification gates pass
Given this scope,
When formatting, linting, and targeted tests run,
Then `cargo fmt --check`, `cargo clippy -p tau-provider -- -D warnings`, and targeted provider/coding-agent tests pass.

## Scope

### In Scope
- `tau-provider` secret wrapper and trait API additions.
- File-backed `SecretStore` implementation over current credential-store load/save behavior.
- Auto mode resolution and key-material derivation update for machine fallback.
- Conformance and regression tests tied to ACs.

### Out of Scope
- Introducing new encryption dependencies (`aes-gcm`, `redb`) in this phase.
- Cross-crate migration of all auth flows to trait-only interfaces.
- Credential store schema version bump.

## Conformance Cases
- C-01 (unit): `DecryptedSecret` redacts `Debug` and `Display` output.
- C-02 (functional): file-backed `SecretStore` integration-secret write/read roundtrip succeeds.
- C-03 (conformance): auto mode without explicit key resolves to keyed behavior and encoded payload does not contain plaintext.
- C-04 (regression): keyed payload decryption with incorrect passphrase fails integrity checks.
- C-05 (regression): plaintext (`none`) mode and legacy payload compatibility remain loadable.
- C-06 (verify): scoped fmt/clippy/targeted test gates pass.

## Success Metrics / Observable Signals
- New public secret wrapper/trait APIs are exported and test-covered.
- Auto-mode stores no plaintext when no explicit key is provided.
- Existing keyed-passphrase and legacy plaintext tests remain green.
