# Tasks: Issue #2652 - SecretStore contract, DecryptedSecret redaction, and machine-auto encryption (G20 phase 1)

## Ordered Tasks
1. T1 (RED): add failing conformance tests for C-01..C-03 covering wrapper redaction, trait-backed file roundtrip, and auto-mode keyed behavior.
2. T2 (GREEN): implement `DecryptedSecret` and `SecretStore`/`FileSecretStore` in `tau-provider`.
3. T3 (GREEN): update auto encryption mode + machine-derived fallback key derivation while preserving explicit `none` and keyed passphrase behavior.
4. T4 (REGRESSION): update/extend coding-agent provider credential-store tests for new auto behavior and keyed/none compatibility.
5. T5 (VERIFY): run scoped fmt/clippy/targeted tests and collect AC-to-test evidence.
6. T6 (CLOSE): update roadmap issue evidence, PR artifacts, and closure status.

## Tier Mapping
- Unit: C-01
- Property: N/A (no randomized invariant API added in this phase)
- Contract/DbC: N/A (contracts crate not used in this module)
- Snapshot: N/A (behavior asserted directly)
- Functional: C-02, C-03
- Conformance: C-01..C-06
- Integration: C-02, C-05
- Fuzz: N/A (no parser/untrusted input surface change)
- Mutation: N/A (non-critical-path incremental hardening)
- Regression: C-04, C-05
- Performance: N/A (no hotspot/perf target change)
