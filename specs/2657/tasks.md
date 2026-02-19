# Tasks: Issue #2657 - Encrypted API-key persistence via SecretStore (G20 phase 2)

## Ordered Tasks
1. T1 (RED): add failing tests for C-01..C-03 proving plaintext-at-rest API-key persistence is blocked and encrypted retrieval is available.
2. T2 (GREEN): wire API-key persistence/retrieval paths to SecretStore-backed encrypted credential-store handling.
3. T3 (REGRESSION): preserve explicit none/keyed compatibility and existing auth fallback behavior.
4. T4 (VERIFY): run scoped fmt/clippy/tests and capture AC mapping evidence.
5. T5 (CLOSE): update roadmap + issue closeout artifacts.

## Tier Mapping
- Unit: C-01
- Property: N/A (no randomized invariant API)
- Contract/DbC: N/A (contracts crate not used for touched surface)
- Snapshot: N/A (no snapshot artifact)
- Functional: C-02
- Conformance: C-01..C-05
- Integration: C-02, C-04
- Fuzz: N/A (no new untrusted parser)
- Mutation: N/A (non-critical-path incremental migration)
- Regression: C-03, C-04
- Performance: N/A (no perf contract change)
