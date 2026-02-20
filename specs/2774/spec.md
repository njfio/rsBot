# Spec: Issue #2774 - G20 crypto/store dependency closure and AES-GCM hardening

Status: Implemented

## Problem Statement
`tasks/spacebot-comparison.md` still has one unchecked G20 pathway row for dependency/backend hardening. Current keyed credential-store encryption in `tau-provider` uses a custom XOR+SHA stream/tag construction, which is weaker and non-standard compared to an AEAD primitive.

## Acceptance Criteria

### AC-1 Approved G20 dependencies are declared with explicit scope
Given the workspace dependency graph,
When #2774 is implemented,
Then `aes-gcm` and `redb` are declared in approved scope with no unrelated dependency churn.

### AC-2 Keyed secret encryption uses AES-256-GCM for new payloads
Given keyed credential-store mode,
When secrets are persisted,
Then encrypted payloads are written with authenticated AES-GCM format and decrypt successfully with matching key material.

### AC-3 Backward compatibility for prior credential payloads is preserved
Given existing `enc:v1:` credential payloads already on disk,
When #2774 ships,
Then decrypt/load paths continue to read legacy payloads without migration breakage.

### AC-4 Regression behavior remains stable for provider/integration flows
Given current provider and integration auth flows,
When encryption internals are hardened,
Then scoped regression tests remain green with no API contract drift.

### AC-5 Roadmap evidence is reconciled
Given implementation completion,
When verification passes,
Then G20 dependency pathway row is checked with `#2774` evidence.

## Scope

### In Scope
- Workspace dependency declarations for `aes-gcm` and `redb`.
- `tau-provider` keyed encryption/decryption hardening to AES-GCM for new writes.
- Legacy decrypt compatibility path for existing `enc:v1:` payloads.
- Checklist/spec evidence updates.

### Out of Scope
- Redesigning credential-store file format beyond encryption envelope/versioning.
- Migrating secret storage backend to redb in this slice.
- CI/CD Fly.io pipeline changes (handled under G23).

## Conformance Cases
- C-01 (conformance): dependency declarations present and scoped to intended manifests.
- C-02 (functional): keyed encrypt/decrypt roundtrip emits v2 envelope and returns original plaintext.
- C-03 (regression): legacy `enc:v1:` payload decrypt succeeds.
- C-04 (regression): tampered keyed payload fails closed with integrity/auth failure.
- C-05 (integration): provider/integration auth flows using credential store remain green in scoped tests.
- C-06 (docs): G20 row checked in roadmap with `#2774` evidence.

## Success Metrics / Observable Signals
- No unchecked G20 dependency row remains in roadmap.
- New keyed payloads are authenticated-encryption encoded.
- Existing encrypted stores continue loading without manual intervention.

## Approval Gate
This task introduced new dependencies (`aes-gcm`, `redb`) and proceeded under explicit user direction to continue contract execution end-to-end.
