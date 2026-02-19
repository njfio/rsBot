# M107 - Spacebot G20 Secret Store Hardening (Phase 1)

Status: Completed
Related roadmap items: `tasks/spacebot-comparison.md` (G20)

## Objective
Deliver phase-1 encrypted secret-store hardening in `tau-provider` without introducing new dependencies by adding a reusable secret-store contract, redacted decrypted-secret handling, and machine-derived auto-encryption key fallback.

## Issue Map
- Epic: #2650
- Story: #2651
- Task: #2652

## Deliverables
- Add `DecryptedSecret` wrapper type with redacted `Debug`/`Display` rendering.
- Add `SecretStore` trait and file-backed implementation over existing credential-store load/save primitives.
- Update credential-store auto encryption mode to default to keyed encryption with machine-derived key material when no passphrase is provided.
- Preserve existing credential-store schema compatibility and explicit keyed-passphrase flows.
- Add conformance/regression tests for wrapper redaction, trait-backed secret roundtrip, and auto-mode behavior.

## Exit Criteria
- #2650, #2651, and #2652 are closed.
- `specs/2652/spec.md` status is `Implemented`.
- `tasks/spacebot-comparison.md` G20 progress is updated with issue evidence.
- Scoped `fmt`/`clippy`/targeted tests pass with AC-to-test mapping in PR evidence.
