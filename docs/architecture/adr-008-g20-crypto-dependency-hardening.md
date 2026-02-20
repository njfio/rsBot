# ADR-008: G20 Crypto Dependency Hardening for Credential Store

## Context
G20 in `tasks/spacebot-comparison.md` had one remaining unchecked dependency/backend row:

1. Add `aes-gcm` and `redb` dependencies (or use existing rusqlite).

`tau-provider` credential-store keyed mode previously used a custom XOR+SHA stream/tag envelope (`enc:v1:`). While this path already enforced fail-closed integrity checks, it did not use a standard AEAD primitive.

## Decision
1. Add workspace dependency declarations for:
   - `aes-gcm` (crypto hardening primitive)
   - `redb` (approved storage backend dependency posture for future backend evolution)
2. Update `tau-provider` keyed secret-store envelope for new writes to AES-256-GCM (`enc:v2:`).
3. Preserve backward compatibility by continuing to decrypt legacy `enc:v1:` payloads.
4. Keep storage backend migration out of scope for this slice; `redb` is declared but not yet adopted as runtime persistence backend.

## Consequences
### Positive
- Keyed secret payloads now use authenticated encryption with a standard AEAD implementation.
- Existing encrypted credential stores remain readable without forced migration.
- G20 dependency checklist is fully reconciled with issue evidence.

### Negative
- Dependency graph grows due to `aes-gcm` transitive crypto crates.
- Two envelope versions (`enc:v1:` and `enc:v2:`) must be maintained during transition.

### Neutral / Follow-on
- Future work can migrate persistence backend to `redb` (or existing `rusqlite`) in a dedicated slice.
- Legacy `enc:v1:` support can be sunset in a later planned migration once upgrade coverage is sufficient.
