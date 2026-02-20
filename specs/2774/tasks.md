# Tasks: Issue #2774 - G20 crypto/store dependency closure and AES-GCM hardening

## Ordered Tasks
1. [x] T1 (RED): add failing tests for v2 keyed envelope roundtrip, legacy v1 compatibility, and tamper rejection.
2. [x] T2 (GREEN): declare `aes-gcm` and `redb` dependencies in approved manifests.
3. [x] T3 (GREEN): implement AES-256-GCM keyed encryption for new payloads with legacy v1 decrypt fallback.
4. [x] T4 (REGRESSION): run scoped tau-provider/provider-auth regression tests and fix drift.
5. [x] T5 (VERIFY): run fmt/clippy/tests and capture evidence.
6. [x] T6 (DOC): update G20 roadmap row with `#2774` evidence and mark spec implemented.

## Tier Mapping
- Unit: credential encryption/decryption behavior tests
- Property: N/A (no randomized invariant harness in this slice)
- Contract/DbC: N/A (no contract macro changes)
- Snapshot: N/A
- Functional: keyed roundtrip + fail-closed behavior
- Conformance: C-01..C-06
- Integration: provider/integration credential flow tests
- Fuzz: N/A
- Mutation: N/A (crypto envelope hardening slice; no new decision graph requiring mutants in this step)
- Regression: legacy payload compatibility + existing provider tests
- Performance: N/A
