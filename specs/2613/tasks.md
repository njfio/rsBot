# Tasks: Issue #2613 - Encrypted gateway auth secret migration

## Ordered Tasks
1. T1 (RED): add gateway validation and startup transport tests covering token/password secret-id behavior.
2. T2 (GREEN): add CLI auth token/password ID fields and validation support.
3. T3 (GREEN): implement credential-store ID resolution for gateway auth with fail-closed propagation.
4. T4 (GREEN): update gateway remote profile checks/plans for ID-backed auth configuration.
5. T5 (GREEN): update gateway operator docs with migration + rotation procedure.
6. T6 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-onboarding -p tau-cli -- -D warnings`, `cargo test -p tau-onboarding gateway_openresponses`, and `cargo test -p tau-cli gateway_remote_profile`.
7. T7 (CLOSE): update issue process log with AC/test evidence and open PR.

## Tier Mapping
- Unit: C-01
- Property: N/A (no property-based harness required for this configuration-path slice)
- Contract/DbC: N/A (no new `contracts` annotations)
- Snapshot: N/A (no snapshot fixtures)
- Functional: C-02
- Conformance: C-01..C-05
- Integration: C-04
- Fuzz: N/A (no new parser/untrusted binary surface)
- Mutation: N/A (non-critical configuration-path slice; covered by targeted regression tests)
- Regression: C-03
- Performance: N/A (no hotspot/perf baseline change)
